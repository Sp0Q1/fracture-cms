//! Demo job executors for the reference app.
//!
//! `ContentStatsJob` counts the org's projects and notes each run and
//! reports changes since the previous completed run — a minimal example of
//! the `JobExecutor` + diff-tracking contract that consuming apps implement
//! for their own job types.

use std::error::Error;

use fracture_core::jobs::{JobDiff, JobExecutor, JobRegistry, JobResult};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter};
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

/// Builds the app's job registry. Called once at startup.
#[must_use]
pub fn build_registry() -> JobRegistry {
    let mut registry = JobRegistry::new();
    registry.register(Box::new(ContentStatsJob));
    registry
}
