//! Configurable, staff-managed permission thresholds for the jobs feature.
//!
//! Job actions fall into three buckets — **view**, **run**, and **manage**
//! (create / edit / delete / enable-disable). Each bucket has a configurable
//! minimum [`JobAccessLevel`]; a request is allowed when the actor's org role
//! clears that level, or when the actor is platform staff (the staff ceiling
//! clears every level). Platform staff configure the thresholds; they live in
//! the platform-admin org's settings, so the whole deployment shares one
//! policy and no new table is needed.

use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use crate::models::org_members::OrgRole;
use crate::models::organizations;

/// Settings key under which the policy is stored on the staff org.
const SETTINGS_KEY: &str = "job_permissions";

/// The minimum standing required for a job action. Ordered from most open
/// (`Viewer`) to most restrictive (`Staff`, i.e. platform staff only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobAccessLevel {
    /// Any org member (Viewer or above).
    Viewer,
    /// Member or above.
    Member,
    /// Admin or above.
    Admin,
    /// Owner only (within the org).
    Owner,
    /// Platform staff only — no tenant role clears it.
    Staff,
}

impl JobAccessLevel {
    /// Whether an actor with `role` (and possibly `is_staff`) clears this level.
    /// Staff clear every level; otherwise the org role must rank high enough.
    #[must_use]
    pub const fn allows(self, role: OrgRole, is_staff: bool) -> bool {
        if is_staff {
            return true;
        }
        match self {
            Self::Viewer => role.at_least(OrgRole::Viewer),
            Self::Member => role.at_least(OrgRole::Member),
            Self::Admin => role.at_least(OrgRole::Admin),
            Self::Owner => role.at_least(OrgRole::Owner),
            Self::Staff => false,
        }
    }

    /// All five levels in display order — used to render the staff settings
    /// dropdowns.
    #[must_use]
    pub const fn all() -> [Self; 5] {
        [
            Self::Viewer,
            Self::Member,
            Self::Admin,
            Self::Owner,
            Self::Staff,
        ]
    }

    /// Stable string form (matches the serde representation).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Member => "member",
            Self::Admin => "admin",
            Self::Owner => "owner",
            Self::Staff => "staff",
        }
    }

    /// Human label for the settings UI.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Viewer => "Any member (Viewer+)",
            Self::Member => "Member or above",
            Self::Admin => "Admin or above",
            Self::Owner => "Owner only",
            Self::Staff => "Platform staff only",
        }
    }

    /// Parses the stored/submitted string, falling back to `default`.
    #[must_use]
    pub fn from_str_or(s: &str, default: Self) -> Self {
        match s {
            "viewer" => Self::Viewer,
            "member" => Self::Member,
            "admin" => Self::Admin,
            "owner" => Self::Owner,
            "staff" => Self::Staff,
            _ => default,
        }
    }
}

/// The full job-permission policy: one threshold per action bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobPermissions {
    /// Minimum to list/see jobs and runs.
    pub view: JobAccessLevel,
    /// Minimum to trigger a run.
    pub run: JobAccessLevel,
    /// Minimum to create / edit / delete / enable-disable.
    pub manage: JobAccessLevel,
}

impl Default for JobPermissions {
    /// Tenant view-only: any member can look, but running and managing jobs is
    /// platform-staff only.
    fn default() -> Self {
        Self {
            view: JobAccessLevel::Viewer,
            run: JobAccessLevel::Staff,
            manage: JobAccessLevel::Staff,
        }
    }
}

/// Resolved booleans for one actor, for gating handlers and toggling UI.
#[derive(Debug, Clone, Copy)]
pub struct JobAccess {
    pub can_view: bool,
    pub can_run: bool,
    pub can_manage: bool,
}

impl JobPermissions {
    /// Loads the policy from the platform-admin org's settings, falling back to
    /// [`Default`] when unset, malformed, or when no staff org exists yet.
    pub async fn load(db: &DatabaseConnection) -> Self {
        let Ok(Some(staff_org)) = organizations::Model::find_staff_org(db).await else {
            return Self::default();
        };
        staff_org
            .get_setting(SETTINGS_KEY)
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default()
    }

    /// Persists the policy onto the platform-admin org's settings.
    ///
    /// # Errors
    ///
    /// Returns an error if no staff org exists or the settings write fails.
    pub async fn save(self, db: &DatabaseConnection) -> Result<(), sea_orm::DbErr> {
        let staff_org = organizations::Model::find_staff_org(db)
            .await?
            .ok_or_else(|| sea_orm::DbErr::Custom("no platform-admin organization".into()))?;
        let value =
            serde_json::to_value(self).map_err(|e| sea_orm::DbErr::Custom(e.to_string()))?;
        organizations::Model::set_setting(db, staff_org.id, SETTINGS_KEY, value).await
    }

    /// Resolves the policy to concrete booleans for an actor.
    #[must_use]
    pub const fn access(self, role: OrgRole, is_staff: bool) -> JobAccess {
        JobAccess {
            can_view: self.view.allows(role, is_staff),
            can_run: self.run.allows(role, is_staff),
            can_manage: self.manage.allows(role, is_staff),
        }
    }
}
