# Authority & Capability Model

How the framework decides what an actor may do with a specific resource.
The engine is `fracture_core::permissions` (`resolve(...)`); it is
domain-independent — apps supply a per-resource-type policy.

## The tier ladder

```
platform / cross-tenant admin   ← ceiling, always full, never capped
        ≥ local Owner ≥ Admin ≥ Member ≥ Viewer
```

The ordering is **monotonic**: a higher tier is never *less* privileged than
a lower tier on the same resource. The resolver enforces this — you never get
"a member can edit but the admin can't".

- **Platform admins** (`is_platform_admin`) short-circuit to every
  capability. They are the ceiling and are never capped by a resource policy.
  (In the federated model this is the brokered staff member; see
  [FEDERATION.md](FEDERATION.md).)
- **Resource owner** — whoever created a resource has full control of it
  (the "a user CRUDs their own documents" direction).
- **Everyone else** gets what the resource's policy grants their tier.

## Owner tier — the both-directions switch

Each resource records who created it: `OwnerTier::Org` (a member) or
`OwnerTier::Platform` (staff). The policy keys off this:

- **Org-owned**: the policy can grant local tiers real authority (members
  edit, admins delete, …).
- **Platform-owned**: the policy caps the local tiers *downward* — a
  staff-created doc can give even the local **Owner** only `{view, comment}`.
  This is the "staff creates, local members and even admins can only view and
  comment" direction.

Both directions are the same machinery with a different per-type policy.

## Capabilities

Open strings, so domain code defines named actions beyond the built-ins
(`view`, `comment`, `edit`, `delete`). Example: a form might define
`submit_intake`; the resolver decides *who may* `submit_intake`, and the
handler for that action enforces the fine print (e.g. "only this one
field"). **Field-level rules live in the handler, not the resolver.**

## Per-user grants

`resource_assignments` (the existing table) grants a specific user extra
capabilities on a specific resource. The `role_key` is opaque to the
framework — the app maps it to capability strings and passes those to
`resolve(..., granted)`. Grants stack on top of the tier default; they are
issued by someone who outranks the target, so they never breach the ceiling.

## Resolution order (in `resolve`)

1. Platform admin → `All`.
2. Actor owns this resource → `All`.
3. Otherwise: union the policy's capabilities for every tier at or below the
   actor's role (this is what enforces monotonicity), then add the actor's
   per-user grants.

## Using it

```rust
use fracture_core::permissions::{resolve, Actor, OwnerTier, ResourcePolicy, VIEW, COMMENT, EDIT, DELETE};

struct DocPolicy;
impl ResourcePolicy for DocPolicy {
    fn caps_for(&self, owner: OwnerTier, role: OrgRole) -> Vec<&'static str> {
        match (owner, role) {
            (OwnerTier::Org,      OrgRole::Member)             => vec![VIEW, COMMENT, EDIT],
            (OwnerTier::Org,      OrgRole::Admin | OrgRole::Owner) => vec![VIEW, COMMENT, EDIT, DELETE],
            (OwnerTier::Platform, _)                           => vec![VIEW, COMMENT], // capped
            _                                                  => vec![VIEW],
        }
    }
}

let caps = resolve(&actor, owner_tier, &DocPolicy, &granted);
if caps.allows(EDIT) { /* ... */ }
```

Controllers ask the resolver (`caps.allows(...)`) instead of the blanket
`require_role!` for resources that need per-resource authority. `require_role!`
remains correct for org-wide gates (e.g. "is this user an org admin at all").

## Status

The engine and its tests ship in `fracture-core`. Wiring it into a concrete
resource (recording `OwnerTier`, defining the policy, calling `resolve` in
the controller) is done per resource type by the consuming app.
