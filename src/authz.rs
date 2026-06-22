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
use fracture_core::permissions::{Authorizable, OwnerTier, ResourcePolicy, COMMENT, VIEW};

use crate::models::_entities::{note_comments, notes};

/// `resource_type` key for note grants in `resource_assignments`.
pub const NOTE: &str = "note";

/// Capability policy for notes.
///
/// Clients can **read and comment** but not edit/delete the note itself; staff
/// (platform admin) are the ceiling and bypass the policy, so they edit/delete
/// anything. `COMMENT` is what gates posting a comment (see `CommentPolicy`).
///
/// This is the per-model knob an app tunes. Capabilities are open strings, so a
/// form-style resource could grant a custom `"submit"` action the same way, or
/// add `EDIT` for org Members to let clients edit their org's own notes.
#[derive(Default)]
pub struct NotePolicy;

impl ResourcePolicy for NotePolicy {
    fn caps_for(&self, _owner: OwnerTier, role: OrgRole) -> Vec<&'static str> {
        match role {
            // Viewers are read-only; Members and up may comment on a note.
            OrgRole::Viewer => vec![VIEW],
            _ => vec![VIEW, COMMENT],
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

/// `resource_type` key for note-comment grants in `resource_assignments`.
pub const NOTE_COMMENT: &str = "note_comment";

/// Capability policy for note comments.
///
/// Everyone in the org gets `VIEW`. Edit/delete are intentionally *not* granted
/// by tier — they come from the resolver's built-ins: a comment's **author**
/// controls their own (creator → full control), and **staff** (platform admin)
/// edit/delete any (the ceiling). So org users can only touch their own
/// comments, exactly as intended.
#[derive(Default)]
pub struct CommentPolicy;

impl ResourcePolicy for CommentPolicy {
    fn caps_for(&self, _owner: OwnerTier, _role: OrgRole) -> Vec<&'static str> {
        vec![VIEW]
    }
}

impl Authorizable for note_comments::Model {
    type Policy = CommentPolicy;

    fn resource_type() -> &'static str {
        NOTE_COMMENT
    }

    fn resource_id(&self) -> i32 {
        self.id
    }

    fn owner_tier(&self) -> OwnerTier {
        // Comment ownership doesn't distinguish tiers; author/staff rules
        // (handled by the resolver) decide who may edit or delete.
        OwnerTier::Org
    }

    fn created_by(&self) -> Option<i32> {
        Some(self.author_id)
    }
}
