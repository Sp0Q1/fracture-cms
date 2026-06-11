use std::fmt;

use sea_orm::entity::prelude::*;
use sea_orm::{QuerySelect, TransactionTrait};

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

    /// Finds all members of an organization paired with their user records,
    /// using a single batched user lookup instead of one query per member.
    ///
    /// # Errors
    ///
    /// Returns an error if either query fails.
    pub async fn find_members_with_users(
        db: &DatabaseConnection,
        org_id: i32,
    ) -> Result<Vec<(Self, super::users::Model)>, DbErr> {
        let members = Self::find_members(db, org_id).await?;
        let user_ids: Vec<i32> = members.iter().map(|m| m.user_id).collect();
        let mut users_by_id: std::collections::HashMap<i32, super::users::Model> =
            super::_entities::users::Entity::find()
                .filter(super::_entities::users::Column::Id.is_in(user_ids))
                .all(db)
                .await?
                .into_iter()
                .map(|u| (u.id, u))
                .collect();
        Ok(members
            .into_iter()
            .filter_map(|m| users_by_id.remove(&m.user_id).map(|u| (m, u)))
            .collect())
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
        // Run the last-owner check and the write in one transaction with the
        // owner rows locked, so two concurrent demotions can't both observe
        // owner_count == 2 and leave the org ownerless (FOR UPDATE on Postgres;
        // SQLite serializes writers).
        let txn = db.begin().await?;
        if current_role == OrgRole::Owner && new_role != OrgRole::Owner {
            Self::guard_not_last_owner(&txn, membership.org_id, "demote").await?;
        }
        let mut active: ActiveModel = membership.into();
        active.role = sea_orm::ActiveValue::Set(new_role.to_string());
        let updated = active.update(&txn).await?;
        txn.commit().await?;
        Ok(updated)
    }

    /// Errors with a "last owner" message if the org has one or fewer owners.
    /// Locks the owner rows for the duration of the surrounding transaction.
    async fn guard_not_last_owner<C>(conn: &C, org_id: i32, action: &str) -> Result<(), DbErr>
    where
        C: ConnectionTrait,
    {
        let mut query = Entity::find()
            .filter(Column::OrgId.eq(org_id))
            .filter(Column::Role.eq("owner"));
        // `FOR UPDATE` row locking is only meaningful (and only valid SQL) on
        // PostgreSQL. SQLite has no row locks — its single-writer transaction
        // model already serializes the count-then-write within this txn.
        if conn.get_database_backend() == sea_orm::DatabaseBackend::Postgres {
            query = query.lock_exclusive();
        }
        let owners = query.all(conn).await?;
        if owners.len() <= 1 {
            return Err(DbErr::Custom(format!(
                "Cannot {action} the last owner of an organization"
            )));
        }
        Ok(())
    }

    /// Removes a member from an organization.
    ///
    /// # Errors
    ///
    /// Returns an error if the member is the last owner or the database
    /// operation fails.
    pub async fn remove_member(db: &DatabaseConnection, membership: Self) -> Result<(), DbErr> {
        let role = OrgRole::from_str_role(&membership.role).unwrap_or(OrgRole::Viewer);
        let txn = db.begin().await?;
        if role == OrgRole::Owner {
            Self::guard_not_last_owner(&txn, membership.org_id, "remove").await?;
        }
        membership.delete(&txn).await?;
        txn.commit().await?;
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
