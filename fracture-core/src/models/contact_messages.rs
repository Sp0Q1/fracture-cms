use sea_orm::entity::prelude::*;
use sea_orm::QueryOrder;
use sea_orm::QuerySelect;

pub use super::_entities::contact_messages::{ActiveModel, Column, Entity, Model};
pub type ContactMessages = Entity;

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
    /// Stores a new contact message.
    ///
    /// # Errors
    ///
    /// Returns an error if the database insert fails.
    pub async fn create(
        db: &DatabaseConnection,
        name: &str,
        email: &str,
        message: &str,
    ) -> Result<Self, DbErr> {
        ActiveModel {
            name: sea_orm::ActiveValue::Set(name.to_string()),
            email: sea_orm::ActiveValue::Set(email.to_string()),
            message: sea_orm::ActiveValue::Set(message.to_string()),
            ..Default::default()
        }
        .insert(db)
        .await
    }

    /// Finds a message by its public ID.
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

    /// Returns the most recent messages, newest first.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_recent(db: &DatabaseConnection, limit: u64) -> Result<Vec<Self>, DbErr> {
        Entity::find()
            .order_by_desc(Column::CreatedAt)
            .limit(limit)
            .all(db)
            .await
    }
}

impl ActiveModel {}

impl Entity {}
