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
use fracture_core::permissions::{Authorizable, OwnerTier, ResourcePolicy, DELETE, EDIT, VIEW};

use crate::models::_entities::notes;

/// `resource_type` key for note grants in `resource_assignments`.
pub const NOTE: &str = "note";

/// Capability policy for notes:
/// - **Org-owned** (a client member created it): viewers read; members also
///   edit; admins/owners also delete.
/// - **Staff-owned** (a platform admin created it): the local tiers — *including
///   the org Owner* — can only read. Clients can't edit staff notes.
#[derive(Default)]
pub struct NotePolicy;

impl ResourcePolicy for NotePolicy {
    // The org-Viewer and staff-owned arms both yield `[VIEW]` today but model
    // distinct cases; keep them spelled out so the policy table reads clearly.
    #[allow(clippy::match_same_arms)] // Reason: explicit policy rows, not a copy bug.
    fn caps_for(&self, owner: OwnerTier, role: OrgRole) -> Vec<&'static str> {
        match (owner, role) {
            (OwnerTier::Org, OrgRole::Viewer) => vec![VIEW],
            (OwnerTier::Org, OrgRole::Member) => vec![VIEW, EDIT],
            (OwnerTier::Org, OrgRole::Admin | OrgRole::Owner) => vec![VIEW, EDIT, DELETE],
            // Staff-owned: even the local Owner is capped to read-only.
            (OwnerTier::Staff, _) => vec![VIEW],
        }
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
