pub use super::_entities::movies::{ActiveModel, Column, Entity, Model};
use sea_orm::entity::prelude::*;
use sea_orm::sea_query::Order;
use sea_orm::QueryOrder;
pub type Movies = Entity;

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, _db: &C, insert: bool) -> std::result::Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        if !insert && self.updated_at.is_unchanged() {
            let mut this = self;
            this.updated_at = sea_orm::ActiveValue::Set(chrono::Utc::now().into());
            Ok(this)
        } else {
            Ok(self)
        }
    }
}

impl Model {
    pub async fn find_by_user(db: &DatabaseConnection, user_id: i32) -> Vec<Self> {
        Entity::find()
            .filter(Column::UserId.eq(user_id))
            .order_by(Column::Id, Order::Desc)
            .all(db)
            .await
            .unwrap_or_default()
    }

    pub async fn find_by_id_and_user(
        db: &DatabaseConnection,
        id: i32,
        user_id: i32,
    ) -> Option<Self> {
        Entity::find_by_id(id)
            .filter(Column::UserId.eq(user_id))
            .one(db)
            .await
            .ok()
            .flatten()
    }
}

impl ActiveModel {}

impl Entity {}
