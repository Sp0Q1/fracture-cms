# Adding a New Org-Scoped Resource

Follow this checklist when adding a new resource (e.g., "tasks", "reports", "assets") to your Fracture CMS fork.

## 1. Migration

Create `migration/src/m20YYMMDD_NNNNNN_create_<resource>.rs`:

```rust
// Required columns: id (PkAuto), pid (UUID unique), org_id (FK to organizations)
// Add your domain columns
// Add foreign key to organizations with ON DELETE CASCADE
// Add unique index on pid
```

Register in `migration/src/lib.rs`.

## 2. Entity

Create `src/models/_entities/<resource>.rs`:

- Define `Model` struct with `DeriveEntityModel`
- Add `Relation::Organizations` (belongs_to)
- Add any parent relations (e.g., belongs_to projects)
- Implement `Related<>` traits

Update `src/models/_entities/mod.rs` and `prelude.rs`.

## 3. Model Logic

Create `src/models/<resource>.rs`:

```rust
pub use super::_entities::<resource>::{ActiveModel, Column, Entity, Model};

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, _db: &C, insert: bool) -> Result<Self, DbErr> {
        let mut this = self;
        if insert {
            this.pid = sea_orm::ActiveValue::Set(Uuid::new_v4());
        } else if this.updated_at.is_unchanged() {
            this.updated_at = sea_orm::ActiveValue::Set(chrono::Utc::now().into());
        }
        Ok(this)
    }
}

impl Model {
    pub async fn find_by_org(db, org_id) -> Vec<Self> { /* scoped query */ }
    pub async fn find_by_pid_and_org(db, pid, org_id) -> Option<Self> { /* scoped lookup */ }
}

// Pin org-scoping at the type level — required for every org-owned entity.
// Once this is implemented, `Entity::find_in_org(org_id)` works for free.
impl super::OrgScoped for Entity {
    fn org_id_column() -> Self::Column {
        Column::OrgId
    }
}
```

Register in `src/models/mod.rs`.

## 4. Controller

Create `src/controllers/<resource>.rs`:

- Import RBAC macros: `use crate::{require_role, require_user};`
- Import middleware: `use crate::controllers::middleware::{get_current_user, get_org_context_or_default, OrgRole};`
- Use `require_user!` macro for authentication
- Use `get_org_context_or_default()` for org resolution
- Use `require_role!` macro for authorization
- Scope all queries by `org_ctx.org.id`
- Define `routes()` function

Register in `src/controllers/mod.rs` and `src/app.rs`.

## 5. View

Create `src/views/<resource>.rs`:

- Accept `user`, `org_ctx`, `user_orgs` params
- Use `crate::views::base_context()` for common template vars (re-exported from fracture-core)
- Add resource-specific data to context

Register in `src/views/mod.rs`.

## 6. Templates

Create `assets/views/<resource>/`:

- `list.html` — Table/grid with items, role-gated "New" button
- `create.html` — Form, breadcrumb
- `show.html` — Detail view, role-gated edit/delete buttons
- `edit.html` — Edit form, role-gated delete button

All templates extend `base.html`. Follow these conventions:

- **No inline CSS** — use oat.ink utility classes (`.mt-4`, `.mb-6`, `.hstack`, `.vstack`)
- **No inline JavaScript** — use `data-` attributes handled by `app.js`
- **No `| escape` filter** — Tera auto-escapes `.html` files by default
- **Role-gate** create/edit/delete buttons with `{% if user_role == "member" or ... %}`

## 7. Route Registration

In `src/app.rs`:

```rust
.add_route(controllers::<resource>::routes())
```

## 8. JavaScript Behavior (optional)

If your templates need interactive behavior (copy buttons, auto-submit selects, etc.), add `data-` attributes to the HTML and handle them in `assets/static/app.js`. Never use inline event handlers (`onclick`, `onchange`, etc.) — the CSP blocks them.

## 9. Mailer (optional)

If your resource needs email notifications, create a mailer in `src/mailers/`:

```
src/mailers/
  <resource>.rs              # Mailer struct implementing loco_rs::mailer::Mailer
  <resource>/<action>/
    subject.t                # Tera template for subject line
    html.t                   # Tera template for HTML body
    text.t                   # Tera template for plain text body
```

Register in `src/mailers/mod.rs`.

## 10. Tests

Create `tests/models/<resource>.rs`:

- Test CRUD operations scoped by org_id
- Test cross-org isolation
- Test pid generation on insert

Register in `tests/models/mod.rs`.

## 11. Truncation Order

Update `truncate()` in `src/app.rs` — child tables before parent tables (foreign key order).

## 12. Per-resource capabilities (optional)

Org-wide `require_role!` is enough for resources where the org role decides
access. For resources that need *per-resource* authority — different access
depending on who created the row, or per-user grants — wire in the capability
resolver (`fracture_core::permissions`). The demo `projects` resource is the
reference implementation; see `src/authz.rs` and `docs/PERMISSIONS.md`.

The recipe:

1. **Record ownership.** Add `owner_tier` (`"org"` / `"platform"`) and
   `created_by` columns (migration `m20260622_000001_add_authority_to_projects.rs`).
   On create, set `created_by` to the actor and `owner_tier` to `"platform"`
   when a platform admin creates it, else `"org"`.
2. **Define the policy** — implement `ResourcePolicy::caps_for(owner, role)`
   for your type (`src/authz.rs::ProjectPolicy`), stating what each org tier
   gets in each ownership direction. Staff-owned rows can cap local tiers
   (even Owner) down to e.g. `view`+`comment`.
3. **Resolve in the controller** — fetch the row, then
   `let caps = authz::project_capabilities(db, user.id, org_ctx.is_platform_admin, org_ctx.role, &item).await?;`
   and gate each handler: `if !caps.allows(EDIT) { return Err(Error::NotFound); }`
   (404, not 403 — don't leak existence). Pass `caps` to the view so the
   template hides actions the user can't take.
4. **Per-user grants** — `resource_assignments` rows for `(user, resource)`
   add capabilities on top of the tier default; the helper reads them
   (treating the opaque `role_key` as a capability string).

Field-level rules (e.g. "may submit only this one field") live in that
action's handler, gated on a named capability — not in the resolver.
