use sea_orm::entity::prelude::*;
use sea_orm::QueryOrder;

pub use super::_entities::job_run_diffs::{ActiveModel, Column, Entity, Model};
pub type JobRunDiffs = Entity;

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {}

impl Model {
    /// Returns all diffs for a job run, ordered by id.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_by_run(db: &DatabaseConnection, job_run_id: i32) -> Result<Vec<Self>, DbErr> {
        Entity::find()
            .filter(Column::JobRunId.eq(job_run_id))
            .order_by_asc(Column::Id)
            .all(db)
            .await
    }

    /// Persists the diffs produced by a job execution for the given run.
    ///
    /// # Errors
    ///
    /// Returns an error if the database insert fails.
    pub async fn insert_for_run(
        db: &DatabaseConnection,
        job_run_id: i32,
        diffs: &[crate::jobs::JobDiff],
    ) -> Result<(), DbErr> {
        if diffs.is_empty() {
            return Ok(());
        }
        let models = diffs.iter().map(|d| ActiveModel {
            job_run_id: sea_orm::ActiveValue::Set(job_run_id),
            diff_type: sea_orm::ActiveValue::Set(d.diff_type.clone()),
            entity_key: sea_orm::ActiveValue::Set(d.entity_key.clone()),
            old_value: sea_orm::ActiveValue::Set(d.old_value.as_ref().map(ToString::to_string)),
            new_value: sea_orm::ActiveValue::Set(d.new_value.as_ref().map(ToString::to_string)),
            ..Default::default()
        });
        Entity::insert_many(models).exec(db).await?;
        Ok(())
    }
}

impl ActiveModel {}

impl Entity {}
