//! Authority/capability resolver.
//!
//! Resolves what an actor may do with a specific resource, combining three
//! inputs under a strict, monotonic tier ladder:
//!
//! ```text
//! platform / cross-tenant admin   ← ceiling, always full
//!         ≥ local Owner ≥ Admin ≥ Member ≥ Viewer
//! ```
//!
//! - **Platform admins** (`is_platform_admin`) short-circuit to every
//!   capability — they are the ceiling and are never capped by a resource.
//! - **The resource's owner tier** decides how far the *local* tiers reach:
//!   a staff/platform-created resource can grant local Owners/Admins as
//!   little as `{view, comment}`, while an org-created one can grant members
//!   full CRUD. This is the both-directions behaviour — the per-resource
//!   policy modulates the local tiers *downward*, but never lifts a lower
//!   tier above a higher one (monotonicity is enforced here, not left to the
//!   policy author).
//! - **Per-user grants** (from `resource_assignments`) add capabilities to a
//!   specific user on a specific resource, on top of their tier default.
//!
//! Capabilities are open strings so domain code can define named actions
//! (e.g. `"approve"`, `"submit_intake"`) beyond the built-ins; field-level
//! rules live in the handler for that named action, not here.

use std::collections::BTreeSet;

use crate::models::org_members::OrgRole;

/// Built-in capabilities. Domain code may use any additional string.
pub const VIEW: &str = "view";
pub const COMMENT: &str = "comment";
pub const EDIT: &str = "edit";
pub const DELETE: &str = "delete";

/// Who created a resource — sets the baseline authority for the local tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerTier {
    /// Created by platform/cross-tenant staff. Local tiers get only what the
    /// policy explicitly grants (often read/comment).
    Platform,
    /// Created within the org by a member. Local tiers get the org policy.
    Org,
}

/// The actor's standing for this request.
#[derive(Debug, Clone, Copy)]
pub struct Actor {
    /// Cross-tenant/platform admin — the ceiling tier.
    pub is_platform_admin: bool,
    /// The actor's org-wide role.
    pub role: OrgRole,
    /// True if this actor created/owns *this specific* resource.
    pub owns_resource: bool,
}

/// The resolved capability set for an actor on a resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Capabilities {
    /// Every capability (platform admin, or the resource's own creator).
    All,
    /// Exactly this set.
    Only(BTreeSet<String>),
}

impl Capabilities {
    /// Whether `cap` is permitted.
    #[must_use]
    pub fn allows(&self, cap: &str) -> bool {
        match self {
            Self::All => true,
            Self::Only(set) => set.contains(cap),
        }
    }
}

/// Per-resource-type policy, implemented in code by the consuming app.
///
/// Returns the capabilities each org tier gets by default, given who owns
/// the resource. The framework does not define any resource type's policy.
/// Implementations only state each tier's own grant; the resolver unions
/// across the ladder so monotonicity holds even if an implementation forgets
/// to make a higher tier a superset of a lower one.
pub trait ResourcePolicy {
    /// Capabilities granted to exactly `role` on a resource owned by
    /// `owner` (before the monotonic union, per-user grants, and the
    /// platform/owner short-circuits).
    fn caps_for(&self, owner: OwnerTier, role: OrgRole) -> Vec<&'static str>;
}

const LADDER: [OrgRole; 4] = [
    OrgRole::Viewer,
    OrgRole::Member,
    OrgRole::Admin,
    OrgRole::Owner,
];

/// Resolves the actor's effective capabilities on a resource.
///
/// `granted` are the capability strings this specific user holds on this
/// specific resource via `resource_assignments` (the caller maps opaque
/// `role_key`s to capabilities, since their meaning is domain-defined).
#[must_use]
pub fn resolve(
    actor: &Actor,
    owner: OwnerTier,
    policy: &dyn ResourcePolicy,
    granted: &[String],
) -> Capabilities {
    // Ceiling tier: never capped by a resource policy.
    if actor.is_platform_admin {
        return Capabilities::All;
    }
    // You fully control what you created (the "user CRUDs their own docs"
    // direction). A platform-owned resource's creator is platform and was
    // already handled above, so this only lifts an org member on their own row.
    if actor.owns_resource {
        return Capabilities::All;
    }
    let mut set = BTreeSet::new();
    // Monotonic union: everything every tier at or below the actor's gets.
    for role in LADDER {
        if actor.role.at_least(role) {
            for cap in policy.caps_for(owner, role) {
                set.insert(cap.to_string());
            }
        }
    }
    // Per-user grants stack on top (never exceed the actor's tier ceiling in
    // practice, because grants are issued by someone who outranks the target).
    for cap in granted {
        set.insert(cap.clone());
    }
    Capabilities::Only(set)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Example policy exercising both directions:
    /// - Org-owned: members edit, viewers read, everyone comments.
    /// - Platform-owned (staff docs): local tiers get only view+comment,
    ///   *including* Owner/Admin — the downward cap.
    struct DemoPolicy;
    impl ResourcePolicy for DemoPolicy {
        fn caps_for(&self, owner: OwnerTier, role: OrgRole) -> Vec<&'static str> {
            match (owner, role) {
                (OwnerTier::Org, OrgRole::Viewer) => vec![VIEW],
                (OwnerTier::Org, OrgRole::Member) => vec![VIEW, COMMENT, EDIT],
                (OwnerTier::Org, OrgRole::Admin | OrgRole::Owner) => {
                    vec![VIEW, COMMENT, EDIT, DELETE]
                }
                // Staff-owned: even local Owner is capped to read + comment.
                (OwnerTier::Platform, _) => vec![VIEW, COMMENT],
            }
        }
    }

    fn actor(pa: bool, role: OrgRole, owns: bool) -> Actor {
        Actor {
            is_platform_admin: pa,
            role,
            owns_resource: owns,
        }
    }

    #[test]
    fn platform_admin_is_the_ceiling() {
        let caps = resolve(
            &actor(true, OrgRole::Viewer, false),
            OwnerTier::Platform,
            &DemoPolicy,
            &[],
        );
        assert_eq!(caps, Capabilities::All);
        assert!(caps.allows(DELETE) && caps.allows("any_custom_action"));
    }

    #[test]
    fn resource_owner_has_full_control_of_own_resource() {
        let caps = resolve(
            &actor(false, OrgRole::Member, true),
            OwnerTier::Org,
            &DemoPolicy,
            &[],
        );
        assert_eq!(caps, Capabilities::All);
    }

    #[test]
    fn staff_owned_caps_local_owner_to_view_and_comment() {
        // The second direction: staff creates, even the local Owner only
        // views and comments.
        let caps = resolve(
            &actor(false, OrgRole::Owner, false),
            OwnerTier::Platform,
            &DemoPolicy,
            &[],
        );
        assert!(caps.allows(VIEW) && caps.allows(COMMENT));
        assert!(!caps.allows(EDIT) && !caps.allows(DELETE));
    }

    #[test]
    fn org_owned_gives_members_edit_not_delete() {
        let caps = resolve(
            &actor(false, OrgRole::Member, false),
            OwnerTier::Org,
            &DemoPolicy,
            &[],
        );
        assert!(caps.allows(EDIT));
        assert!(!caps.allows(DELETE));
    }

    #[test]
    fn ladder_is_monotonic_admin_gets_at_least_member() {
        let member = resolve(
            &actor(false, OrgRole::Member, false),
            OwnerTier::Org,
            &DemoPolicy,
            &[],
        );
        let admin = resolve(
            &actor(false, OrgRole::Admin, false),
            OwnerTier::Org,
            &DemoPolicy,
            &[],
        );
        for cap in [VIEW, COMMENT, EDIT] {
            assert!(member.allows(cap) && admin.allows(cap));
        }
        assert!(admin.allows(DELETE) && !member.allows(DELETE));
    }

    #[test]
    fn per_user_grant_adds_a_named_action() {
        // A viewer normally can't comment on a staff doc... here they're
        // granted a domain action explicitly.
        let granted = vec!["approve".to_string()];
        let caps = resolve(
            &actor(false, OrgRole::Viewer, false),
            OwnerTier::Platform,
            &DemoPolicy,
            &granted,
        );
        assert!(caps.allows("approve"));
        assert!(caps.allows(VIEW)); // tier default still applies
        assert!(!caps.allows(EDIT));
    }
}
