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
}

impl ActiveModel {}

impl Entity {}
