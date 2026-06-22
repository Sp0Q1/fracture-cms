//! Reference example: wiring `fracture_core::permissions` into a concrete
//! resource (the demo `projects`).
//!
//! This is the per-resource glue a downstream app writes for each resource
//! type it wants capability-based access on: a [`ResourcePolicy`] (what each
//! tier may do, in each ownership direction) and a small helper that builds
//! the [`Actor`], reads per-user grants from `resource_assignments`, and
//! calls [`resolve`]. Controllers then gate on the returned [`Capabilities`]
//! instead of a blanket `require_role!`. See `docs/ADDING_RESOURCES.md`.

use fracture_core::models::org_members::OrgRole;
use fracture_core::models::resource_assignments;
use fracture_core::permissions::{
    resolve, Actor, Capabilities, OwnerTier, ResourcePolicy, COMMENT, DELETE, EDIT, VIEW,
};
use sea_orm::DatabaseConnection;

use crate::models::_entities::projects;

/// `resource_type` key for project assignments in `resource_assignments`.
pub const PROJECT: &str = "project";

/// Capability policy for projects, demonstrating both directions:
/// - **Org-owned** (a member created it): viewers read; members also comment
///   and edit; admins/owners also delete.
/// - **Platform-owned** (staff/platform-admin created it): the local tiers —
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
            (OwnerTier::Platform, _) => vec![VIEW, COMMENT],
        }
    }
}

fn owner_tier(project: &projects::Model) -> OwnerTier {
    if project.owner_tier == "platform" {
        OwnerTier::Platform
    } else {
        OwnerTier::Org
    }
}

/// Resolves what `user_id` may do with `project`.
///
/// Folds in any per-user grants from `resource_assignments` (a grant's
/// opaque `role_key` is treated as a capability string — the app defines
/// that mapping).
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
        owner_tier(project),
        &ProjectPolicy,
        &granted,
    ))
}
