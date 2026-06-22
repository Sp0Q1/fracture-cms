//! Per-resource authorization glue: wires `fracture_core::permissions` into the
//! demo resources (`projects`, `notes`).
//!
//! For each resource type an app wants capability-based access on, it writes a
//! [`ResourcePolicy`]. `notes` uses the newer [`Authorizable`] trait +
//! `middleware::capabilities` + `require_capability!`; `projects` shows the
//! lower-level `resolve` helper. See `docs/ADDING_RESOURCES.md`.

use fracture_core::models::org_members::OrgRole;
use fracture_core::models::resource_assignments;
use fracture_core::permissions::{
    resolve, Actor, Authorizable, Capabilities, OwnerTier, ResourcePolicy, COMMENT, DELETE, EDIT,
    VIEW,
};
use sea_orm::DatabaseConnection;

use crate::models::_entities::{notes, projects};

/// `resource_type` key for project assignments in `resource_assignments`.
pub const PROJECT: &str = "project";

/// Capability policy for projects, demonstrating both directions:
/// - **Org-owned** (a member created it): viewers read; members also comment
///   and edit; admins/owners also delete.
/// - **Staff-owned** (a staff/platform admin created it): the local tiers —
///   *including the org Owner* — are capped at view + comment.
pub struct ProjectPolicy;

impl ResourcePolicy for ProjectPolicy {
    fn caps_for(&self, owner: OwnerTier, role: OrgRole) -> Vec<&'static str> {
        match (owner, role) {
            (OwnerTier::Org, OrgRole::Viewer) => vec![VIEW],
            (OwnerTier::Org, OrgRole::Member) => vec![VIEW, COMMENT, EDIT],
            (OwnerTier::Org, OrgRole::Admin | OrgRole::Owner) => {
                vec![VIEW, COMMENT, EDIT, DELETE]
            }
            // Staff-owned: even the local Owner only views and comments.
            (OwnerTier::Staff, _) => vec![VIEW, COMMENT],
        }
    }
}

fn project_owner_tier(project: &projects::Model) -> OwnerTier {
    // Projects store "platform" for staff-created rows (original demo
    // convention); treat that as the Staff tier.
    if project.owner_tier == "platform" || project.owner_tier == "staff" {
        OwnerTier::Staff
    } else {
        OwnerTier::Org
    }
}

/// Resolves what `user_id` may do with `project`.
///
/// Folds in any per-user grants from `resource_assignments` (a grant's opaque
/// `role_key` is treated as a capability string — the app defines that mapping).
///
/// # Errors
///
/// Returns an error if reading the grants fails.
pub async fn project_capabilities(
    db: &DatabaseConnection,
    user_id: i32,
    is_platform_admin: bool,
    role: OrgRole,
    project: &projects::Model,
) -> Result<Capabilities, sea_orm::DbErr> {
    let granted: Vec<String> =
        resource_assignments::Model::list_for_resource(db, PROJECT, project.id)
            .await?
            .into_iter()
            .filter(|a| a.user_id == user_id)
            .map(|a| a.role_key)
            .collect();
    let actor = Actor {
        is_platform_admin,
        role,
        owns_resource: project.created_by == Some(user_id),
    };
    Ok(resolve(
        &actor,
        project_owner_tier(project),
        &ProjectPolicy,
        &granted,
    ))
}

/// `resource_type` key for note grants in `resource_assignments`.
pub const NOTE: &str = "note";

/// Capability policy for notes.
///
/// Org (client) users get **read-only** access by default; staff (platform
/// admin) are the ceiling and bypass the policy, so they edit/delete anything.
/// This is the per-model knob an app tunes (add capabilities per `(owner, role)`
/// to let org users do more — comment, edit, custom actions).
#[derive(Default)]
pub struct NotePolicy;

impl ResourcePolicy for NotePolicy {
    fn caps_for(&self, _owner: OwnerTier, _role: OrgRole) -> Vec<&'static str> {
        // Read-only for every org role, on both org- and staff-owned notes.
        vec![VIEW]
    }
}

impl Authorizable for notes::Model {
    type Policy = NotePolicy;

    fn resource_type() -> &'static str {
        NOTE
    }

    fn resource_id(&self) -> i32 {
        self.id
    }

    fn owner_tier(&self) -> OwnerTier {
        if self.owner_tier == "staff" {
            OwnerTier::Staff
        } else {
            OwnerTier::Org
        }
    }

    fn created_by(&self) -> Option<i32> {
        self.created_by
    }
}
