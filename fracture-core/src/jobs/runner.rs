//! Background runner that executes queued job runs and enqueues runs for
//! cron-scheduled definitions.
//!
//! The runner is a polling loop spawned by [`JobRunnerInitializer`]. Each
//! tick it (1) enqueues a run for every enabled, scheduled definition whose
//! cron expression is due, then (2) claims and executes queued runs one at a
//! time via the registered [`JobExecutor`](super::JobExecutor)s, persisting
//! the outcome (`completed` + summary + diffs, or `failed` + error) on the
//! `job_runs` row.
//!
//! Executors must not panic: a panic aborts the runner task and queued runs
//! stop being processed until restart. Return an `Err` instead — it is
//! recorded on the run as a failure.

use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use loco_rs::app::{AppContext, Initializer};
use loco_rs::Result;
use sea_orm::{DatabaseConnection, EntityTrait};

use super::{try_job_registry, JobRegistry};
use crate::models::{job_definitions, job_run_diffs, job_runs};

const DEFAULT_POLL_INTERVAL_SECS: u64 = 15;

/// Spawns the job runner loop once the app's routes are built.
///
/// Configured via `settings.jobs` in the app config:
///
/// ```yaml
/// settings:
///   jobs:
///     enabled: true              # default true
///     poll_interval_seconds: 15  # default 15
/// ```
///
/// The consuming app must call
/// [`init_job_registry`](super::init_job_registry) (typically in its
/// `routes()` hook) before this initializer runs; otherwise queued runs are
/// marked failed with a "no executor registered" error.
pub struct JobRunnerInitializer;

#[async_trait]
impl Initializer for JobRunnerInitializer {
    fn name(&self) -> String {
        "job-runner".to_string()
    }

    async fn after_routes(&self, router: Router, ctx: &AppContext) -> Result<Router> {
        let settings = ctx.config.settings.as_ref().and_then(|s| s.get("jobs"));
        let enabled = settings
            .and_then(|j| j.get("enabled"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        if !enabled {
            tracing::info!("job runner disabled via settings.jobs.enabled");
            return Ok(router);
        }
        let interval = settings
            .and_then(|j| j.get("poll_interval_seconds"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(DEFAULT_POLL_INTERVAL_SECS)
            .max(1);

        // The app registers executors in its `routes()` hook, which runs
        // before initializers — so an empty slot here means it never will.
        // Fall back to an empty registry: queued runs then fail loudly
        // ("no executor registered") instead of sitting queued forever.
        if try_job_registry().is_none() {
            tracing::warn!(
                "JobRegistry not initialised; job runs will fail with 'no executor registered'"
            );
            super::init_job_registry(JobRegistry::new());
        }
        let registry = super::job_registry();

        let db = ctx.db.clone();
        tokio::spawn(async move {
            tracing::info!(interval_seconds = interval, "job runner started");
            loop {
                if let Err(e) = tick(&db, registry).await {
                    tracing::error!(error = %e, "job runner tick failed");
                }
                tokio::time::sleep(Duration::from_secs(interval)).await;
            }
        });
        Ok(router)
    }
}

/// One runner iteration: enqueue due scheduled runs, then drain the queue.
///
/// # Errors
///
/// Returns an error if a database operation fails. Executor failures do not
/// error — they are recorded on the run.
pub async fn tick(
    db: &DatabaseConnection,
    registry: &JobRegistry,
) -> std::result::Result<(), sea_orm::DbErr> {
    enqueue_due_schedules(db).await?;
    process_queued(db, registry).await
}

/// Enqueues a run for every enabled, scheduled definition that is due.
///
/// A definition is due when its cron expression has an occurrence between
/// its last run and now. Definitions that already have a queued or running
/// run are skipped, so a slow job cannot pile up a backlog of its own runs.
///
/// # Errors
///
/// Returns an error if a database operation fails.
pub async fn enqueue_due_schedules(
    db: &DatabaseConnection,
) -> std::result::Result<(), sea_orm::DbErr> {
    let definitions = job_definitions::Model::find_scheduled(db).await?;
    let now = chrono::Utc::now();
    for definition in definitions {
        let Some(schedule) = definition.schedule.as_deref() else {
            continue;
        };
        let last = job_runs::Model::find_latest_by_definition(db, definition.id).await?;
        let last_at = last.map(|r| r.created_at.with_timezone(&chrono::Utc));
        if !is_due(schedule, last_at, now) {
            continue;
        }
        if job_runs::Model::has_active_run(db, definition.id).await? {
            continue;
        }
        tracing::info!(job = %definition.name, schedule, "schedule due; enqueueing run");
        job_runs::Model::create_queued(db, definition.id, definition.org_id).await?;
    }
    Ok(())
}

/// Returns true when `schedule` has an occurrence at or before `now`.
///
/// `schedule` is a cron expression with a seconds field, e.g.
/// `0 0 * * * *` for hourly; occurrences are counted from the last run. A
/// definition that has never run is due immediately. An unparseable
/// expression is never due (creation validates expressions, but rows can
/// predate that or be edited directly).
#[must_use]
pub fn is_due(
    schedule: &str,
    last_run_at: Option<chrono::DateTime<chrono::Utc>>,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    let Ok(parsed) = cron::Schedule::from_str(schedule) else {
        tracing::warn!(schedule, "unparseable cron schedule; skipping");
        return false;
    };
    last_run_at.is_none_or(|last| parsed.after(&last).next().is_some_and(|next| next <= now))
}

/// Claims and executes queued runs until the queue is empty.
///
/// # Errors
///
/// Returns an error if a database operation fails.
pub async fn process_queued(
    db: &DatabaseConnection,
    registry: &JobRegistry,
) -> std::result::Result<(), sea_orm::DbErr> {
    while let Some(run) = job_runs::Model::claim_oldest_queued(db).await? {
        execute_run(db, registry, run).await?;
    }
    Ok(())
}

/// Executes one claimed (`running`) run and persists the outcome. Executor
/// errors are recorded on the run as failures, never propagated.
///
/// # Errors
///
/// Returns an error if a database operation fails.
pub async fn execute_run(
    db: &DatabaseConnection,
    registry: &JobRegistry,
    run: job_runs::Model,
) -> std::result::Result<(), sea_orm::DbErr> {
    let Some(definition) = job_definitions::Entity::find_by_id(run.job_definition_id)
        .one(db)
        .await?
    else {
        job_runs::Model::mark_failed(db, run, "job definition no longer exists").await?;
        return Ok(());
    };
    // A run queued before its definition was disabled must not execute.
    if !definition.enabled {
        job_runs::Model::mark_failed(db, run, "job definition is disabled").await?;
        return Ok(());
    }
    let Some(executor) = registry.get(&definition.job_type) else {
        let msg = format!(
            "no executor registered for job type '{}'",
            definition.job_type
        );
        job_runs::Model::mark_failed(db, run, &msg).await?;
        return Ok(());
    };
    let previous = job_runs::Model::find_latest_completed_by_definition(db, definition.id).await?;
    match executor.execute(db, &definition, previous.as_ref()).await {
        Ok(result) => {
            let run_id = run.id;
            job_run_diffs::Model::insert_for_run(db, run_id, &result.diffs).await?;
            job_runs::Model::mark_completed(db, run, &result.summary).await?;
            tracing::info!(
                job = %definition.name,
                run = run_id,
                diffs = result.diffs.len(),
                "job run completed"
            );
        }
        Err(e) => {
            tracing::warn!(job = %definition.name, error = %e, "job run failed");
            job_runs::Model::mark_failed(db, run, &e.to_string()).await?;
        }
    }
    Ok(())
}
