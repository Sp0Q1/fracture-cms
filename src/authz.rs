//! Per-resource authorization glue: wires `fracture_core::permissions` into a
//! concrete resource (the demo `notes`).
//!
//! For each resource type an app wants capability-based access on, it writes
//! exactly two things here — a [`ResourcePolicy`] (what each org tier may do,
//! given who owns the resource) and an [`Authorizable`] impl (where the owner
//! tier, creator, and grant key live). The framework's
//! `middleware::capabilities` + `require_capability!` then do the rest; no
//! per-resource helper and no inline authorization checks in controllers.
//! See `docs/ADDING_RESOURCES.md`.

use fracture_core::models::org_members::OrgRole;
use fracture_core::permissions::{Authorizable, OwnerTier, ResourcePolicy, VIEW};

use crate::models::_entities::notes;

/// `resource_type` key for note grants in `resource_assignments`.
pub const NOTE: &str = "note";

/// Capability policy for notes.
///
/// Org (client) users get **read-only** access by default; staff (platform
/// admin) are the ceiling and bypass the policy, so they edit/delete anything.
///
/// This is the per-model knob an app tunes. To give org users more on a
/// resource, return extra capabilities per `(owner, role)` — for example:
///
/// ```ignore
/// match (owner, role) {
///     (OwnerTier::Org, OrgRole::Member) => vec![VIEW, COMMENT, EDIT],
///     (OwnerTier::Org, OrgRole::Admin | OrgRole::Owner) => vec![VIEW, COMMENT, EDIT, DELETE],
///     (OwnerTier::Org, _) => vec![VIEW, COMMENT],
///     (OwnerTier::Staff, _) => vec![VIEW],   // clients never edit staff content
/// }
/// ```
///
/// Capabilities are open strings, so a form-style resource can grant a custom
/// `"submit"` action to org members the same way.
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
