use std::fmt;

use sea_orm::entity::prelude::*;

pub use super::_entities::org_members::{ActiveModel, Column, Entity, Model};
pub type OrgMembers = Entity;

/// Role hierarchy: Owner > Admin > Member > Viewer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrgRole {
    Viewer = 0,
    Member = 1,
    Admin = 2,
    Owner = 3,
}

impl OrgRole {
    /// Returns true if this role is at least as powerful as `minimum`.
    #[must_use]
    pub const fn at_least(self, minimum: Self) -> bool {
        (self as u8) >= (minimum as u8)
    }

    /// Parses a role from the string stored in the database.
    #[must_use]
    pub fn from_str_role(s: &str) -> Option<Self> {
        match s {
            "owner" => Some(Self::Owner),
            "admin" => Some(Self::Admin),
            "member" => Some(Self::Member),
            "viewer" => Some(Self::Viewer),
            _ => None,
        }
    }
}

impl fmt::Display for OrgRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Owner => write!(f, "owner"),
            Self::Admin => write!(f, "admin"),
            Self::Member => write!(f, "member"),
            Self::Viewer => write!(f, "viewer"),
        }
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {}

impl Model {
    /// Finds a membership for a user in an organization.
    /// Platform admins get a virtual admin membership if they're not a real member.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_membership_or_admin(
        db: &DatabaseConnection,
        org_id: i32,
        user_id: i32,
    ) -> Result<Option<Self>, DbErr> {
        if let Some(m) = Self::find_membership(db, org_id, user_id).await? {
            return Ok(Some(m));
        }
        if super::organizations::Model::is_user_platform_admin(db, user_id).await {
            return Ok(Some(Self::virtual_admin(org_id, user_id)));
        }
        Ok(None)
    }

    /// Finds a membership for a user in an organization (exact match only).
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_membership(
        db: &DatabaseConnection,
        org_id: i32,
        user_id: i32,
    ) -> Result<Option<Self>, DbErr> {
        Entity::find()
            .filter(Column::OrgId.eq(org_id))
            .filter(Column::UserId.eq(user_id))
            .one(db)
            .await
    }

    /// Creates a virtual (non-persisted) admin membership for platform admins
    /// accessing orgs they are not a member of.
    #[must_use]
    pub fn virtual_admin(org_id: i32, user_id: i32) -> Self {
        Self {
            id: 0,
            created_at: chrono::Utc::now().into(),
            updated_at: chrono::Utc::now().into(),
            org_id,
            user_id,
            role: "owner".to_string(),
        }
    }

    /// Finds all members of an organization.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn find_members(db: &DatabaseConnection, org_id: i32) -> Result<Vec<Self>, DbErr> {
        Entity::find()
            .filter(Column::OrgId.eq(org_id))
            .all(db)
            .await
    }

    /// Adds a member to an organization.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn add_member(
        db: &DatabaseConnection,
        org_id: i32,
        user_id: i32,
        role: OrgRole,
    ) -> Result<Self, DbErr> {
        ActiveModel {
            org_id: sea_orm::ActiveValue::Set(org_id),
            user_id: sea_orm::ActiveValue::Set(user_id),
            role: sea_orm::ActiveValue::Set(role.to_string()),
            ..Default::default()
        }
        .insert(db)
        .await
    }

    /// Updates the role of a membership.
    ///
    /// # Errors
    ///
    /// Returns an error if demoting the last owner or if the database
    /// operation fails.
    pub async fn update_role(
        db: &DatabaseConnection,
        membership: Self,
        new_role: OrgRole,
    ) -> Result<Self, DbErr> {
        let current_role = OrgRole::from_str_role(&membership.role).unwrap_or(OrgRole::Viewer);
        if current_role == OrgRole::Owner && new_role != OrgRole::Owner {
            let owner_count = Entity::find()
                .filter(Column::OrgId.eq(membership.org_id))
                .filter(Column::Role.eq("owner"))
                .count(db)
                .await?;
            if owner_count <= 1 {
                return Err(DbErr::Custom(
                    "Cannot demote the last owner of an organization".to_string(),
                ));
            }
        }
        let mut active: ActiveModel = membership.into();
        active.role = sea_orm::ActiveValue::Set(new_role.to_string());
        active.update(db).await
    }

    /// Removes a member from an organization.
    ///
    /// # Errors
    ///
    /// Returns an error if the member is the last owner or the database
    /// operation fails.
    pub async fn remove_member(db: &DatabaseConnection, membership: Self) -> Result<(), DbErr> {
        let role = OrgRole::from_str_role(&membership.role).unwrap_or(OrgRole::Viewer);
        if role == OrgRole::Owner {
            let owner_count = Entity::find()
                .filter(Column::OrgId.eq(membership.org_id))
                .filter(Column::Role.eq("owner"))
                .count(db)
                .await?;
            if owner_count <= 1 {
                return Err(DbErr::Custom(
                    "Cannot remove the last owner of an organization".to_string(),
                ));
            }
        }
        membership.delete(db).await?;
        Ok(())
    }
}

impl ActiveModel {}

impl Entity {}

impl super::OrgScoped for Entity {
    fn org_id_column() -> Self::Column {
        Column::OrgId
    }
}
