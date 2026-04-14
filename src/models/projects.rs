use sea_orm::entity::prelude::*;
use sea_orm::sea_query::Order;
use sea_orm::QueryOrder;

pub use super::_entities::projects::{ActiveModel, Column, Entity, Model};
pub type Projects = Entity;

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, _db: &C, insert: bool) -> std::result::Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let mut this = self;
        if insert {
            this.pid = sea_orm::ActiveValue::Set(Uuid::new_v4());
        } else if this.updated_at.is_unchanged() {
            this.updated_at = sea_orm::ActiveValue::Set(chrono::Utc::now().into());
        }
        Ok(this)
    }
}

impl Model {
    /// Finds all projects belonging to an organization.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_by_org(db: &DatabaseConnection, org_id: i32) -> Result<Vec<Self>, DbErr> {
        Entity::find()
            .filter(Column::OrgId.eq(org_id))
            .order_by(Column::Id, Order::Desc)
            .all(db)
            .await
    }

    /// Finds a project by pid, scoped to an organization.
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
