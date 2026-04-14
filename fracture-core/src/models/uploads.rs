use sea_orm::entity::prelude::*;
use sea_orm::QueryOrder;

pub use super::_entities::uploads::{ActiveModel, Column, Entity, Model};
pub type Uploads = Entity;

/// Upload visibility levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// Visible only to members of the owning organization.
    Org,
    /// Publicly accessible (e.g. blog images).
    Public,
}

impl Visibility {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Org => "org",
            Self::Public => "public",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "org" => Some(Self::Org),
            "public" => Some(Self::Public),
            _ => None,
        }
    }
}

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
        Ok(this)
    }
}

impl Model {
    /// Finds an upload by its public ID (UUID).
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

    /// Returns all uploads for an organization, newest first.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_by_org(db: &DatabaseConnection, org_id: i32) -> Result<Vec<Self>, DbErr> {
        Entity::find()
            .filter(Column::OrgId.eq(org_id))
            .order_by_desc(Column::CreatedAt)
            .all(db)
            .await
    }
}

impl ActiveModel {}

impl Entity {}
