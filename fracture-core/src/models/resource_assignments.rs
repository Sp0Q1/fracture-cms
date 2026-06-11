//! Generic per-resource role assignments.
//!
//! This is the **only sanctioned mechanism** for downstream crates to grant
//! per-resource access to a user — it is the airtight ownership building
//! block that prevents IDORs at the model layer.
//!
//! `fracture-core` provides the *mechanism* (the table, the lookup helpers,
//! the active/expired/revoked semantics). It does **not** define what role
//! strings mean. Downstream crates own role semantics: a consumer might
//! define `"pentester"` as "may view and edit findings on this engagement".
//!
//! ## Active assignment
//!
//! An assignment is *active* when:
//! - `revoked_at` is `NULL`, AND
//! - `expires_at` is `NULL` OR `expires_at` is in the future.
//!
//! All `has_assignment` / `list_for_resource` / `list_for_user` helpers
//! return only active rows. Use `list_history_for_resource` if you need to
//! see revoked or expired ones (e.g. for audit display).

use chrono::Utc;
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveValue, Condition, QueryOrder};

pub use super::_entities::resource_assignments::{ActiveModel, Column, Entity, Model};
pub type ResourceAssignments = Entity;

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, _db: &C, insert: bool) -> std::result::Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let mut this = self;
        if insert {
            this.pid = ActiveValue::Set(Uuid::new_v4());
            let now = Utc::now().into();
            this.granted_at = match this.granted_at {
                ActiveValue::NotSet => ActiveValue::Set(now),
                other => other,
            };
            this.created_at = match this.created_at {
                ActiveValue::NotSet => ActiveValue::Set(now),
                other => other,
            };
            this.updated_at = ActiveValue::Set(now);
        } else if this.updated_at.is_unchanged() {
            this.updated_at = ActiveValue::Set(Utc::now().into());
        }
        Ok(this)
    }
}

/// Parameters for granting a new assignment. Built explicitly so callers
/// can't accidentally swap `org_id` and `resource_id`.
#[derive(Debug, Clone)]
pub struct AssignParams<'a> {
    pub user_id: i32,
    pub org_id: i32,
    pub resource_type: &'a str,
    pub resource_id: i32,
    pub role_key: &'a str,
    pub granted_by: Option<i32>,
    pub expires_at: Option<DateTimeWithTimeZone>,
}

/// Assignment-related errors callers can pattern-match on.
#[derive(Debug, thiserror::Error)]
pub enum AssignmentError {
    #[error("an active assignment already exists for this (user, resource, role)")]
    AlreadyAssigned,
    #[error("database error: {0}")]
    Db(#[from] DbErr),
}

impl Model {
    /// Returns the `SeaORM` filter that restricts to *active* (not revoked,
    /// not expired) assignments. Centralised so every helper applies the
    /// same definition.
    fn active_filter() -> Condition {
        Condition::all().add(Column::RevokedAt.is_null()).add(
            Condition::any()
                .add(Column::ExpiresAt.is_null())
                .add(Column::ExpiresAt.gt(Utc::now())),
        )
    }

    /// Look up an assignment by its public id (UUID).
    ///
    /// # Errors
    /// Returns an error if the database query fails.
    pub async fn find_by_pid(db: &DatabaseConnection, pid: &str) -> Result<Option<Self>, DbErr> {
        let Ok(uuid) = Uuid::parse_str(pid) else {
            return Ok(None);
        };
        Entity::find().filter(Column::Pid.eq(uuid)).one(db).await
    }

    /// Grant an assignment. Returns `AlreadyAssigned` if an active
    /// assignment already exists for the same (user, resource, role).
    ///
    /// # Errors
    /// Returns `AssignmentError::AlreadyAssigned` on duplicate, or
    /// `AssignmentError::Db` on any other database failure.
    pub async fn assign(
        db: &DatabaseConnection,
        params: AssignParams<'_>,
    ) -> Result<Self, AssignmentError> {
        if Self::has_assignment(
            db,
            params.user_id,
            params.resource_type,
            params.resource_id,
            params.role_key,
        )
        .await?
        {
            return Err(AssignmentError::AlreadyAssigned);
        }

        let am = ActiveModel {
            user_id: ActiveValue::Set(params.user_id),
            org_id: ActiveValue::Set(params.org_id),
            resource_type: ActiveValue::Set(params.resource_type.to_string()),
            resource_id: ActiveValue::Set(params.resource_id),
            role_key: ActiveValue::Set(params.role_key.to_string()),
            granted_by: ActiveValue::Set(params.granted_by),
            expires_at: ActiveValue::Set(params.expires_at),
            ..Default::default()
        };
        Ok(am.insert(db).await?)
    }

    /// Mark an assignment as revoked. Idempotent: revoking an already-revoked
    /// row is a no-op that returns the existing row.
    ///
    /// # Errors
    /// Returns an error if the database query fails.
    pub async fn revoke(self, db: &DatabaseConnection) -> Result<Self, DbErr> {
        if self.revoked_at.is_some() {
            return Ok(self);
        }
        let mut am: ActiveModel = self.into();
        am.revoked_at = ActiveValue::Set(Some(Utc::now().into()));
        am.update(db).await
    }

    /// Returns true iff the user holds an *active* assignment with this role
    /// on this resource. This is the canonical authorization check; downstream
    /// crates wrap it in domain-specific helpers (e.g. `can_edit_findings`).
    ///
    /// # Errors
    /// Returns an error if the database query fails.
    pub async fn has_assignment(
        db: &DatabaseConnection,
        user_id: i32,
        resource_type: &str,
        resource_id: i32,
        role_key: &str,
    ) -> Result<bool, DbErr> {
        let count = Entity::find()
            .filter(Column::UserId.eq(user_id))
            .filter(Column::ResourceType.eq(resource_type))
            .filter(Column::ResourceId.eq(resource_id))
            .filter(Column::RoleKey.eq(role_key))
            .filter(Self::active_filter())
            .count(db)
            .await?;
        Ok(count > 0)
    }

    /// Returns true iff the user holds *any* active assignment on this
    /// resource (any role). Useful for "can the user even see this thing?".
    ///
    /// # Errors
    /// Returns an error if the database query fails.
    pub async fn has_any_assignment(
        db: &DatabaseConnection,
        user_id: i32,
        resource_type: &str,
        resource_id: i32,
    ) -> Result<bool, DbErr> {
        let count = Entity::find()
            .filter(Column::UserId.eq(user_id))
            .filter(Column::ResourceType.eq(resource_type))
            .filter(Column::ResourceId.eq(resource_id))
            .filter(Self::active_filter())
            .count(db)
            .await?;
        Ok(count > 0)
    }

    /// All active assignments on a single resource — i.e. "who has access
    /// to this thing?". Ordered by `granted_at` ascending (oldest first).
    ///
    /// # Errors
    /// Returns an error if the database query fails.
    pub async fn list_for_resource(
        db: &DatabaseConnection,
        resource_type: &str,
        resource_id: i32,
    ) -> Result<Vec<Self>, DbErr> {
        Entity::find()
            .filter(Column::ResourceType.eq(resource_type))
            .filter(Column::ResourceId.eq(resource_id))
            .filter(Self::active_filter())
            .order_by_asc(Column::GrantedAt)
            .all(db)
            .await
    }

    /// All active assignments a user holds on resources of a given type —
    /// i.e. "what engagements does this pentester have?".
    ///
    /// # Errors
    /// Returns an error if the database query fails.
    pub async fn list_for_user(
        db: &DatabaseConnection,
        user_id: i32,
        resource_type: &str,
    ) -> Result<Vec<Self>, DbErr> {
        Entity::find()
            .filter(Column::UserId.eq(user_id))
            .filter(Column::ResourceType.eq(resource_type))
            .filter(Self::active_filter())
            .order_by_desc(Column::GrantedAt)
            .all(db)
            .await
    }

    /// Active resource ids only — convenient for `WHERE id IN (...)` joins.
    ///
    /// # Errors
    /// Returns an error if the database query fails.
    pub async fn assigned_resource_ids(
        db: &DatabaseConnection,
        user_id: i32,
        resource_type: &str,
    ) -> Result<Vec<i32>, DbErr> {
        let rows = Self::list_for_user(db, user_id, resource_type).await?;
        Ok(rows.into_iter().map(|r| r.resource_id).collect())
    }

    /// Full history (active + revoked + expired) for a resource. Use this
    /// for audit-style displays, not for authorization decisions.
    ///
    /// # Errors
    /// Returns an error if the database query fails.
    pub async fn list_history_for_resource(
        db: &DatabaseConnection,
        resource_type: &str,
        resource_id: i32,
    ) -> Result<Vec<Self>, DbErr> {
        Entity::find()
            .filter(Column::ResourceType.eq(resource_type))
            .filter(Column::ResourceId.eq(resource_id))
            .order_by_desc(Column::GrantedAt)
            .all(db)
            .await
    }
}
