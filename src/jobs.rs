//! Demo job executors for the reference app.
//!
//! `ContentStatsJob` counts the org's projects and notes each run and
//! reports changes since the previous completed run — a read-only example of
//! the `JobExecutor` + diff-tracking contract.
//!
//! `WriteNoteJob` is a side-effecting template job: each run writes a note
//! into the definition's org. It is the smallest end-to-end proof that the
//! whole lifecycle — queue, run, persist a change, record a diff, surface it
//! in the UI — works. Consuming apps register their own executors the same
//! way (see [`build_registry`]); CMS imposes no limit on what they do.

use std::error::Error;

use fracture_core::jobs::{JobDiff, JobExecutor, JobRegistry, JobResult};
use fracture_core::listing::{FormField, FormOption};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, DbErr, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder,
};
use serde_json::json;

use crate::models::_entities::{notes, projects};
use fracture_core::models::_entities::{job_definitions, job_runs};

pub struct ContentStatsJob;

#[async_trait::async_trait]
impl JobExecutor for ContentStatsJob {
    fn job_type(&self) -> &'static str {
        "content_stats"
    }

    async fn execute(
        &self,
        db: &DatabaseConnection,
        definition: &job_definitions::Model,
        previous_run: Option<&job_runs::Model>,
    ) -> Result<JobResult, Box<dyn Error + Send + Sync>> {
        let org_id = definition.org_id;
        // The demo entities still hand-roll org scoping (see CLAUDE.md);
        // count via explicit org_id filters like the rest of the demo app.
        let project_count = projects::Entity::find()
            .filter(projects::Column::OrgId.eq(org_id))
            .count(db)
            .await?;
        let note_count = notes::Entity::find()
            .filter(notes::Column::OrgId.eq(org_id))
            .count(db)
            .await?;
        let summary = json!({ "projects": project_count, "notes": note_count });

        let mut diffs = Vec::new();
        if let Some(previous_summary) = previous_run
            .and_then(|run| run.result_summary.as_deref())
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        {
            for key in ["projects", "notes"] {
                let old = previous_summary.get(key).cloned();
                let new = summary.get(key).cloned();
                if old != new {
                    diffs.push(JobDiff {
                        diff_type: "changed".to_string(),
                        entity_key: key.to_string(),
                        old_value: old,
                        new_value: new,
                    });
                }
            }
        }

        Ok(JobResult { summary, diffs })
    }
}

/// A template job that writes a note into the org each run.
///
/// A minimal, side-effecting proof of the job lifecycle: run it manually from
/// a job's page to watch a queued run execute, create a note, and record a
/// `created` diff. It also demonstrates a job-defined config form (see
/// [`config_form`](WriteNoteJob::config_form)) — a project dropdown and an
/// optional title — so org owners create it without touching JSON.
pub struct WriteNoteJob;

#[async_trait::async_trait]
impl JobExecutor for WriteNoteJob {
    fn job_type(&self) -> &'static str {
        "write_note"
    }

    fn label(&self) -> &'static str {
        "Write a note"
    }

    fn description(&self) -> &'static str {
        "Creates a note in a chosen project on each run."
    }

    async fn config_form(
        &self,
        db: &DatabaseConnection,
        org_id: i32,
    ) -> Result<Vec<FormField>, DbErr> {
        // The project dropdown is built from this org's projects, so the form
        // is dynamic per org — the kind of thing a raw JSON textarea can't do.
        let projects = projects::Entity::find()
            .filter(projects::Column::OrgId.eq(org_id))
            .order_by_asc(projects::Column::Title)
            .all(db)
            .await?;
        let options = projects
            .iter()
            .map(|p| FormOption::new(p.pid.to_string(), p.title.clone()))
            .collect();
        Ok(vec![
            FormField::select("project_id", "Project", options)
                .with_help("Where the note will be created."),
            FormField::text("title", "Note title")
                .optional()
                .with_help("Prefix for the note's title. Defaults to “Automated note”."),
        ])
    }

    async fn execute(
        &self,
        db: &DatabaseConnection,
        definition: &job_definitions::Model,
        _previous_run: Option<&job_runs::Model>,
    ) -> Result<JobResult, Box<dyn Error + Send + Sync>> {
        let org_id = definition.org_id;
        let config: serde_json::Value =
            serde_json::from_str(&definition.config).unwrap_or_else(|_| json!({}));

        // Prefer the project chosen in the config form; otherwise fall back to
        // the org's first project, creating a holding project if it has none.
        let chosen_pid = config.get("project_id").and_then(serde_json::Value::as_str);
        let project = match chosen_pid {
            Some(pid) if !pid.is_empty() => projects::Entity::find()
                .filter(projects::Column::OrgId.eq(org_id))
                .filter(
                    projects::Column::Pid
                        .eq(sea_orm::prelude::Uuid::parse_str(pid).unwrap_or_default()),
                )
                .one(db)
                .await?
                .ok_or_else(|| -> Box<dyn Error + Send + Sync> {
                    "the configured project no longer exists".into()
                })?,
            _ => match projects::Entity::find()
                .filter(projects::Column::OrgId.eq(org_id))
                .order_by_asc(projects::Column::Id)
                .one(db)
                .await?
            {
                Some(p) => p,
                None => {
                    projects::ActiveModel {
                        org_id: Set(org_id),
                        title: Set("Automated Notes".to_string()),
                        description: Set(Some(
                            "Holds notes created by the write_note template job.".to_string(),
                        )),
                        owner_tier: Set("org".to_string()),
                        ..Default::default()
                    }
                    .insert(db)
                    .await?
                }
            },
        };

        // Optional title prefix from the config form.
        let prefix = config
            .get("title")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("Automated note");
        let now = chrono::Utc::now();
        let title = format!("{prefix} — {}", now.format("%Y-%m-%d %H:%M:%S UTC"));

        let note = notes::ActiveModel {
            project_id: Set(project.id),
            org_id: Set(org_id),
            title: Set(title.clone()),
            body: Set(Some(format!(
                "Written by the '{}' job at {}.",
                definition.name,
                now.to_rfc3339()
            ))),
            // Org-owned: created by the org's automation, not staff.
            owner_tier: Set("org".to_string()),
            ..Default::default()
        }
        .insert(db)
        .await?;

        let summary = json!({
            "note_pid": note.pid.to_string(),
            "project_pid": project.pid.to_string(),
            "title": title,
        });
        let diffs = vec![JobDiff {
            diff_type: "created".to_string(),
            entity_key: format!("note:{}", note.pid),
            old_value: None,
            new_value: Some(json!({ "title": title })),
        }];
        Ok(JobResult { summary, diffs })
    }
}

/// Builds the app's job registry. Called once at startup.
#[must_use]
pub fn build_registry() -> JobRegistry {
    let mut registry = JobRegistry::new();
    registry.register(Box::new(ContentStatsJob));
    registry.register(Box::new(WriteNoteJob));
    registry
}
