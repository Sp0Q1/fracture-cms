use sea_orm::entity::prelude::*;
use sea_orm::sea_query::Order;
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
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_by_pid(db: &DatabaseConnection, pid: &str) -> Result<Option<Self>, DbErr> {
        let Some(uuid) = Uuid::parse_str(pid).ok() else {
            return Ok(None);
        };
        Entity::find().filter(Column::Pid.eq(uuid)).one(db).await
    }

    /// Returns all job definitions for an org, newest first.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_all_by_org(db: &DatabaseConnection, org_id: i32) -> Result<Vec<Self>, DbErr> {
        Entity::find()
            .filter(Column::OrgId.eq(org_id))
            .order_by(Column::CreatedAt, Order::Desc)
            .all(db)
            .await
    }

    /// Returns all enabled definitions that have a schedule, across orgs.
    /// Used by the job runner to evaluate cron schedules.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_scheduled(db: &DatabaseConnection) -> Result<Vec<Self>, DbErr> {
        Entity::find()
            .filter(Column::Enabled.eq(true))
            .filter(Column::Schedule.is_not_null())
            .all(db)
            .await
    }

    /// Finds a job definition by PID and verifies org ownership.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_by_pid_and_org(
        db: &DatabaseConnection,
        pid: &str,
        org_id: i32,
    ) -> Result<Option<Self>, DbErr> {
        let Some(uuid) = Uuid::parse_str(pid).ok() else {
            return Ok(None);
        };
        Entity::find()
            .filter(Column::Pid.eq(uuid))
            .filter(Column::OrgId.eq(org_id))
            .one(db)
            .await
    }
}

impl ActiveModel {}

impl Entity {}

impl super::OrgScoped for Entity {
    fn org_id_column() -> Self::Column {
        Column::OrgId
    }
}
