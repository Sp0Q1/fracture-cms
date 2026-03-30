use sea_orm::entity::prelude::*;
use sea_orm::QueryOrder;

pub use super::_entities::job_runs::{ActiveModel, Column, Entity, Model};
pub type JobRuns = Entity;

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, _db: &C, insert: bool) -> std::result::Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let mut this = self;
        if insert {
            this.pid = sea_orm::ActiveValue::Set(Uuid::new_v4());
        }

        if !insert && this.updated_at.is_unchanged() {
            this.updated_at = sea_orm::ActiveValue::Set(chrono::Utc::now().into());
        }

        Ok(this)
    }
}

impl Model {
    /// Finds a job run by its public ID.
    pub async fn find_by_pid(db: &DatabaseConnection, pid: &str) -> Option<Self> {
        let uuid = Uuid::parse_str(pid).ok()?;
        Entity::find()
            .filter(Column::Pid.eq(uuid))
            .one(db)
            .await
            .ok()
            .flatten()
    }

    /// Returns all runs for a job definition, newest first.
    pub async fn find_by_definition(db: &DatabaseConnection, definition_id: i32) -> Vec<Self> {
        Entity::find()
            .filter(Column::JobDefinitionId.eq(definition_id))
            .order_by_desc(Column::CreatedAt)
            .all(db)
            .await
            .unwrap_or_default()
    }

    /// Returns the latest completed run for a job definition.
    pub async fn find_latest_completed_by_definition(
        db: &DatabaseConnection,
        definition_id: i32,
    ) -> Option<Self> {
        Entity::find()
            .filter(Column::JobDefinitionId.eq(definition_id))
            .filter(Column::Status.eq("completed"))
            .order_by_desc(Column::CompletedAt)
            .one(db)
            .await
            .ok()
            .flatten()
    }

    /// Creates a new queued job run for the given definition and org.
    ///
    /// # Errors
    ///
    /// Returns an error if the database insert fails.
    pub async fn create_queued(
        db: &DatabaseConnection,
        definition_id: i32,
        org_id: i32,
    ) -> Result<Self, DbErr> {
        ActiveModel {
            job_definition_id: sea_orm::ActiveValue::Set(definition_id),
            org_id: sea_orm::ActiveValue::Set(org_id),
            status: sea_orm::ActiveValue::Set("queued".to_string()),
            ..Default::default()
        }
        .insert(db)
        .await
    }
}

impl ActiveModel {}

impl Entity {}
