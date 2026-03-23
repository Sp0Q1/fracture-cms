use sea_orm::entity::prelude::*;
use sea_orm::sea_query::Order;
use sea_orm::QueryOrder;

pub use super::_entities::notes::{ActiveModel, Column, Entity, Model};
pub type Notes = Entity;

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, _db: &C, insert: bool) -> std::result::Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let mut this = self;
        if insert {
            this.pid = sea_orm::ActiveValue::Set(Uuid::new_v4().to_string());
        } else if this.updated_at.is_unchanged() {
            this.updated_at = sea_orm::ActiveValue::Set(chrono::Utc::now().into());
        }
        Ok(this)
    }
}

impl Model {
    /// Finds all notes for a project, scoped to an organization.
    pub async fn find_by_project_and_org(
        db: &DatabaseConnection,
        project_id: i32,
        org_id: i32,
    ) -> Vec<Self> {
        Entity::find()
            .filter(Column::ProjectId.eq(project_id))
            .filter(Column::OrgId.eq(org_id))
            .order_by(Column::Id, Order::Desc)
            .all(db)
            .await
            .unwrap_or_default()
    }

    /// Finds a note by pid, scoped to an organization.
    pub async fn find_by_pid_and_org(
        db: &DatabaseConnection,
        pid: &str,
        org_id: i32,
    ) -> Option<Self> {
        let _uuid = Uuid::parse_str(pid).ok()?;
        Entity::find()
            .filter(Column::Pid.eq(pid))
            .filter(Column::OrgId.eq(org_id))
            .one(db)
            .await
            .ok()
            .flatten()
    }
}

impl ActiveModel {}

impl Entity {}
