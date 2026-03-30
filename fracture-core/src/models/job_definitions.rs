use sea_orm::entity::prelude::*;
use sea_orm::QueryOrder;

pub use super::_entities::job_definitions::{ActiveModel, Column, Entity, Model};
pub type JobDefinitions = Entity;

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
    /// Finds a job definition by its public ID.
    pub async fn find_by_pid(db: &DatabaseConnection, pid: &str) -> Option<Self> {
        let uuid = Uuid::parse_str(pid).ok()?;
        Entity::find()
            .filter(Column::Pid.eq(uuid))
            .one(db)
            .await
            .ok()
            .flatten()
    }

    /// Returns all job definitions for an org, ordered by name.
    pub async fn find_all_by_org(db: &DatabaseConnection, org_id: i32) -> Vec<Self> {
        Entity::find()
            .filter(Column::OrgId.eq(org_id))
            .order_by_asc(Column::Name)
            .all(db)
            .await
            .unwrap_or_default()
    }

    /// Finds a job definition by PID and verifies org ownership.
    pub async fn find_by_pid_and_org(
        db: &DatabaseConnection,
        pid: &str,
        org_id: i32,
    ) -> Option<Self> {
        let uuid = Uuid::parse_str(pid).ok()?;
        Entity::find()
            .filter(Column::Pid.eq(uuid))
            .filter(Column::OrgId.eq(org_id))
            .one(db)
            .await
            .ok()
            .flatten()
    }
}

impl ActiveModel {}

impl Entity {}
