//! Tests for the jobs runner: execution lifecycle, scheduling, and claiming.

use std::error::Error;

use fracture_cms::app::App;
use fracture_cms::models::{
    organizations,
    users::{self, OidcUserInfo},
};
use fracture_core::jobs::{runner, JobDiff, JobExecutor, JobRegistry, JobResult};
use fracture_core::models::_entities::{job_definitions, job_runs as job_runs_entity};
use fracture_core::models::{job_run_diffs, job_runs};
use loco_rs::testing::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};
use serde_json::json;
use serial_test::serial;

struct OkJob;

#[async_trait::async_trait]
impl JobExecutor for OkJob {
    fn job_type(&self) -> &str {
        "ok_job"
    }

    async fn execute(
        &self,
        _db: &DatabaseConnection,
        _definition: &job_definitions::Model,
        previous_run: Option<&job_runs_entity::Model>,
    ) -> Result<JobResult, Box<dyn Error + Send + Sync>> {
        Ok(JobResult {
            summary: json!({ "count": 42, "had_previous": previous_run.is_some() }),
            diffs: vec![JobDiff {
                diff_type: "added".to_string(),
                entity_key: "widget-1".to_string(),
                old_value: None,
                new_value: Some(json!(1)),
            }],
        })
    }
}

struct FailJob;

#[async_trait::async_trait]
impl JobExecutor for FailJob {
    fn job_type(&self) -> &str {
        "fail_job"
    }

    async fn execute(
        &self,
        _db: &DatabaseConnection,
        _definition: &job_definitions::Model,
        _previous_run: Option<&job_runs_entity::Model>,
    ) -> Result<JobResult, Box<dyn Error + Send + Sync>> {
        Err("boom".into())
    }
}

fn test_registry() -> JobRegistry {
    let mut registry = JobRegistry::new();
    registry.register(Box::new(OkJob));
    registry.register(Box::new(FailJob));
    registry
}

async fn mk_org(db: &DatabaseConnection, suffix: &str) -> organizations::Model {
    let user = users::Model::find_or_create_from_oidc(
        db,
        &OidcUserInfo {
            provider: "test".to_string(),
            subject: format!("test-jobs-{suffix}"),
            email: format!("jobs-{suffix}@example.com"),
            name: Some(format!("Jobs {suffix}")),
            email_verified: true,
        },
    )
    .await
    .expect("create user");
    crate::support::owned_org(db, suffix, user.id).await
}

async fn mk_definition(
    db: &DatabaseConnection,
    org_id: i32,
    job_type: &str,
    schedule: Option<&str>,
    enabled: bool,
) -> job_definitions::Model {
    // job_definitions has a unique (org_id, name) index — derive a distinct
    // name from the parameters.
    job_definitions::ActiveModel {
        org_id: Set(org_id),
        name: Set(format!("test {job_type} {schedule:?} {enabled}")),
        job_type: Set(job_type.to_string()),
        schedule: Set(schedule.map(String::from)),
        enabled: Set(enabled),
        config: Set("{}".to_string()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert definition")
}

#[tokio::test]
#[serial]
async fn successful_run_persists_summary_and_diffs() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;
    let org = mk_org(db, "ok").await;
    let def = mk_definition(db, org.id, "ok_job", None, true).await;

    let run = job_runs::Model::create_queued(db, def.id, org.id)
        .await
        .unwrap();
    runner::process_queued(db, &test_registry()).await.unwrap();

    let run = job_runs::Model::find_by_pid(db, &run.pid.to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.status, "completed");
    assert!(run.started_at.is_some(), "started_at must be stamped");
    assert!(run.completed_at.is_some(), "completed_at must be stamped");
    let summary: serde_json::Value =
        serde_json::from_str(run.result_summary.as_deref().unwrap()).unwrap();
    assert_eq!(summary["count"], 42);
    assert_eq!(summary["had_previous"], false);

    let diffs = job_run_diffs::Model::find_by_run(db, run.id).await.unwrap();
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].entity_key, "widget-1");
    assert_eq!(diffs[0].diff_type, "added");
}

#[tokio::test]
#[serial]
async fn second_run_sees_previous_completed_run() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;
    let org = mk_org(db, "prev").await;
    let def = mk_definition(db, org.id, "ok_job", None, true).await;

    job_runs::Model::create_queued(db, def.id, org.id)
        .await
        .unwrap();
    runner::process_queued(db, &test_registry()).await.unwrap();
    let second = job_runs::Model::create_queued(db, def.id, org.id)
        .await
        .unwrap();
    runner::process_queued(db, &test_registry()).await.unwrap();

    let second = job_runs::Model::find_by_pid(db, &second.pid.to_string())
        .await
        .unwrap()
        .unwrap();
    let summary: serde_json::Value =
        serde_json::from_str(second.result_summary.as_deref().unwrap()).unwrap();
    assert_eq!(summary["had_previous"], true);
}

#[tokio::test]
#[serial]
async fn failing_executor_marks_run_failed() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;
    let org = mk_org(db, "fail").await;
    let def = mk_definition(db, org.id, "fail_job", None, true).await;

    let run = job_runs::Model::create_queued(db, def.id, org.id)
        .await
        .unwrap();
    runner::process_queued(db, &test_registry()).await.unwrap();

    let run = job_runs::Model::find_by_pid(db, &run.pid.to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.status, "failed");
    assert_eq!(run.error_message.as_deref(), Some("boom"));
    assert!(run.completed_at.is_some());
}

#[tokio::test]
#[serial]
async fn unknown_job_type_marks_run_failed() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;
    let org = mk_org(db, "unknown").await;
    let def = mk_definition(db, org.id, "no_such_type", None, true).await;

    let run = job_runs::Model::create_queued(db, def.id, org.id)
        .await
        .unwrap();
    runner::process_queued(db, &test_registry()).await.unwrap();

    let run = job_runs::Model::find_by_pid(db, &run.pid.to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.status, "failed");
    assert!(
        run.error_message
            .unwrap()
            .contains("no executor registered"),
        "error must name the missing executor"
    );
}

#[tokio::test]
#[serial]
async fn run_for_disabled_definition_fails_instead_of_executing() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;
    let org = mk_org(db, "disabled").await;
    let def = mk_definition(db, org.id, "ok_job", None, false).await;

    let run = job_runs::Model::create_queued(db, def.id, org.id)
        .await
        .unwrap();
    runner::process_queued(db, &test_registry()).await.unwrap();

    let run = job_runs::Model::find_by_pid(db, &run.pid.to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.status, "failed");
    assert!(run.error_message.unwrap().contains("disabled"));
}

#[tokio::test]
#[serial]
async fn claim_transitions_a_run_exactly_once() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;
    let org = mk_org(db, "claim").await;
    let def = mk_definition(db, org.id, "ok_job", None, true).await;

    job_runs::Model::create_queued(db, def.id, org.id)
        .await
        .unwrap();

    let first = job_runs::Model::claim_oldest_queued(db).await.unwrap();
    let first = first.expect("first claim wins the queued run");
    assert_eq!(first.status, "running");
    assert!(first.started_at.is_some());

    let second = job_runs::Model::claim_oldest_queued(db).await.unwrap();
    assert!(
        second.is_none(),
        "a running run must not be claimable again"
    );
}

#[tokio::test]
#[serial]
async fn due_schedule_enqueues_exactly_one_run() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;
    let org = mk_org(db, "sched").await;
    // Every second — always due for a never-run definition.
    let def = mk_definition(db, org.id, "ok_job", Some("* * * * * *"), true).await;
    // Disabled and unscheduled definitions must not be picked up.
    mk_definition(db, org.id, "ok_job", Some("* * * * * *"), false).await;
    mk_definition(db, org.id, "ok_job", None, true).await;

    runner::enqueue_due_schedules(db).await.unwrap();
    runner::enqueue_due_schedules(db).await.unwrap(); // second tick: active run → skip

    let runs = job_runs::Model::find_by_definition(db, def.id)
        .await
        .unwrap();
    assert_eq!(runs.len(), 1, "active run must suppress re-enqueueing");
    assert_eq!(runs[0].status, "queued");

    let all_queued = job_runs_entity::Entity::find().all(db).await.unwrap();
    assert_eq!(
        all_queued.len(),
        1,
        "disabled/unscheduled definitions must not be enqueued"
    );
}

#[test]
fn is_due_logic() {
    let now = chrono::Utc::now();
    let hourly = "0 0 * * * *";

    // Never run: due immediately.
    assert!(runner::is_due(hourly, None, now));
    // Ran just now: the next hourly occurrence is in the future.
    assert!(!runner::is_due(hourly, Some(now), now));
    // Ran two hours ago: an occurrence has passed.
    assert!(runner::is_due(
        hourly,
        Some(now - chrono::Duration::hours(2)),
        now
    ));
    // Unparseable expressions are never due, even for never-run definitions.
    assert!(!runner::is_due("not a cron", Some(now), now));
    assert!(!runner::is_due("not a cron", None, now));
}
