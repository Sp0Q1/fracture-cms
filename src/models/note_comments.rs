use sea_orm::entity::prelude::*;
use sea_orm::sea_query::Order;
use sea_orm::QueryOrder;

pub use super::_entities::note_comments::{ActiveModel, Column, Entity, Model};
pub type NoteComments = Entity;

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
    /// Lists a note's comments oldest-first (timeline order), scoped to an org.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_by_note_and_org(
        db: &DatabaseConnection,
        note_id: i32,
        org_id: i32,
    ) -> Result<Vec<Self>, DbErr> {
        Entity::find()
            .filter(Column::NoteId.eq(note_id))
            .filter(Column::OrgId.eq(org_id))
            .order_by(Column::Id, Order::Asc)
            .all(db)
            .await
    }

    /// Finds a comment by pid, scoped to an organization (IDOR-safe lookup).
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_by_pid_and_org(
        db: &DatabaseConnection,
        pid: &str,
        org_id: i32,
    ) -> Result<Option<Self>, DbErr> {
        let Ok(uuid) = Uuid::parse_str(pid) else {
            return Ok(None);
        };
        Entity::find()
            .filter(Column::Pid.eq(uuid))
            .filter(Column::OrgId.eq(org_id))
            .one(db)
            .await
    }
}
