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

/// Why a membership write was refused (or failed).
///
/// Controllers map [`Self::NotFound`] and [`Self::Forbidden`] to 404 (never
/// 403 — see the IDOR policy) and surface [`Self::LastOwner`] as a message.
#[derive(Debug, thiserror::Error)]
pub enum MemberWriteError {
    #[error("membership not found")]
    NotFound,
    #[error("the acting role may not modify this membership")]
    Forbidden,
    #[error("Cannot {0} the last owner of an organization")]
    LastOwner(&'static str),
    #[error(transparent)]
    Db(#[from] DbErr),
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
        Self::find_membership_in(db, org_id, user_id, false).await
    }

    /// Finds a membership on any connection (including an open transaction).
    /// With `lock` set, the row is locked `FOR UPDATE` on `PostgreSQL` so it
    /// cannot change for the duration of the surrounding transaction.
    async fn find_membership_in<C>(
        conn: &C,
        org_id: i32,
        user_id: i32,
        lock: bool,
    ) -> Result<Option<Self>, DbErr>
    where
        C: ConnectionTrait,
    {
        let mut query = Entity::find()
            .filter(Column::OrgId.eq(org_id))
            .filter(Column::UserId.eq(user_id));
        if lock && conn.get_database_backend() == sea_orm::DatabaseBackend::Postgres {
            query = query.lock_exclusive();
        }
        query.one(conn).await
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

    /// Updates the role of the target user's membership, on behalf of an
    /// actor holding `actor_role`.
    ///
    /// The target row is re-fetched (and locked, on `PostgreSQL`) inside the
    /// transaction, and the role ceiling — the actor must outrank or equal
    /// both the target's current role and the granted role — is enforced
    /// against that row. Checking against a row the controller read earlier
    /// would race a concurrent promotion of the target.
    ///
    /// # Errors
    ///
    /// [`MemberWriteError::NotFound`] if the membership does not exist,
    /// [`MemberWriteError::Forbidden`] if `actor_role` is outranked, and
    /// [`MemberWriteError::LastOwner`] when demoting the last owner.
    pub async fn update_role(
        db: &DatabaseConnection,
        org_id: i32,
        target_user_id: i32,
        actor_role: OrgRole,
        new_role: OrgRole,
    ) -> Result<Self, MemberWriteError> {
        let txn = db.begin().await?;
        let membership = Self::find_membership_in(&txn, org_id, target_user_id, true)
            .await?
            .ok_or(MemberWriteError::NotFound)?;
        let current_role = OrgRole::from_str_role(&membership.role).unwrap_or(OrgRole::Viewer);
        if !actor_role.at_least(current_role) || !actor_role.at_least(new_role) {
            return Err(MemberWriteError::Forbidden);
        }
        if current_role == OrgRole::Owner && new_role != OrgRole::Owner {
            Self::guard_not_last_owner(&txn, org_id, "demote").await?;
        }
        let mut active: ActiveModel = membership.into();
        active.role = sea_orm::ActiveValue::Set(new_role.to_string());
        let updated = active.update(&txn).await?;
        txn.commit().await?;
        Ok(updated)
    }

    /// Errors with [`MemberWriteError::LastOwner`] if the org has one or
    /// fewer owners. Locks the owner rows for the duration of the
    /// surrounding transaction.
    async fn guard_not_last_owner<C>(
        conn: &C,
        org_id: i32,
        action: &'static str,
    ) -> Result<(), MemberWriteError>
    where
        C: ConnectionTrait,
    {
        let mut query = Entity::find()
            .filter(Column::OrgId.eq(org_id))
            .filter(Column::Role.eq("owner"));
        // `FOR UPDATE` row locking is only meaningful (and only valid SQL) on
        // PostgreSQL — racing demotions block here and re-read committed
        // state. On SQLite, WAL snapshot isolation makes the second of two
        // racing demotions fail its first write with SQLITE_BUSY_SNAPSHOT
        // instead of committing: a spurious error for that request, but the
        // org can never be left ownerless.
        if conn.get_database_backend() == sea_orm::DatabaseBackend::Postgres {
            query = query.lock_exclusive();
        }
        let owners = query.all(conn).await?;
        if owners.len() <= 1 {
            return Err(MemberWriteError::LastOwner(action));
        }
        Ok(())
    }

    /// Removes the target user's membership, on behalf of an actor holding
    /// `actor_role`. See [`Self::update_role`] for why the target row is
    /// re-fetched inside the transaction.
    ///
    /// # Errors
    ///
    /// [`MemberWriteError::NotFound`] if the membership does not exist,
    /// [`MemberWriteError::Forbidden`] if `actor_role` is outranked, and
    /// [`MemberWriteError::LastOwner`] when removing the last owner.
    pub async fn remove_member(
        db: &DatabaseConnection,
        org_id: i32,
        target_user_id: i32,
        actor_role: OrgRole,
    ) -> Result<(), MemberWriteError> {
        let txn = db.begin().await?;
        let membership = Self::find_membership_in(&txn, org_id, target_user_id, true)
            .await?
            .ok_or(MemberWriteError::NotFound)?;
        let role = OrgRole::from_str_role(&membership.role).unwrap_or(OrgRole::Viewer);
        if !actor_role.at_least(role) {
            return Err(MemberWriteError::Forbidden);
        }
        if role == OrgRole::Owner {
            Self::guard_not_last_owner(&txn, org_id, "remove").await?;
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
