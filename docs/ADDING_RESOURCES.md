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

### Per-resource authorization (capabilities)

`require_role!` gates on the actor's org-wide role. When access also depends on
*who owns a specific record* — e.g. a client (even an org Owner) must not edit
content created by staff — use the capability resolver instead of inline checks.

Ownership is binary: `OwnerTier::Org` (a client created it) or `OwnerTier::Staff`
(a platform admin created it). Wire a resource in three small steps:

1. **Columns**: add `owner_tier` (string, default `"org"`) and `created_by`
   (nullable user id) to the table (migration) and the entity. Set them on
   create: `owner_tier = if org_ctx.is_platform_admin { "staff" } else { "org" }`,
   `created_by = Some(user.id)`.
2. **Policy + `Authorizable`** (in `src/authz.rs`): one match table maps
   *(owner tier, role) → capabilities*; the `Authorizable` impl tells the
   framework where `owner_tier`/`created_by`/`resource_id` live. Staff (platform
   admin) and a record's own creator bypass the policy (full control), so the
   table only governs other org users.

   The bundled `notes` resource ships **read-only for clients** (`caps_for`
   returns just `vec![VIEW]`). The example below is the *richer* form — grant
   org users more per role, including custom capability strings for
   form/comment-style resources:

   ```rust
   #[derive(Default)]
   pub struct MyResourcePolicy;
   impl ResourcePolicy for MyResourcePolicy {
       fn caps_for(&self, owner: OwnerTier, role: OrgRole) -> Vec<&'static str> {
           match (owner, role) {
               (OwnerTier::Org, OrgRole::Member) => vec![VIEW, COMMENT, EDIT],
               (OwnerTier::Org, OrgRole::Admin | OrgRole::Owner) => vec![VIEW, COMMENT, EDIT, DELETE],
               (OwnerTier::Org, OrgRole::Viewer) => vec![VIEW, COMMENT],
               (OwnerTier::Staff, _) => vec![VIEW],   // clients can't edit staff content
           }
       }
   }
   impl Authorizable for my_resource::Model {
       type Policy = MyResourcePolicy;
       fn resource_type() -> &'static str { "my_resource" }
       fn resource_id(&self) -> i32 { self.id }
       fn owner_tier(&self) -> OwnerTier { /* map the string column */ }
       fn created_by(&self) -> Option<i32> { self.created_by }
   }
   ```

3. **Gate in the controller** — resolve, then enforce with the macro (never an
   inline `if`):

   ```rust
   let caps = middleware::capabilities(&ctx.db, &org_ctx, user.id, &item).await?;
   require_capability!(caps, EDIT);
   ```

The resolver also folds in per-user grants from `resource_assignments` (a
grant's `role_key` maps to capabilities; override `Authorizable::grant_capabilities`
to expand a named grant like `"reviewer"` → `["view", "comment"]`), and
short-circuits to all capabilities for platform admins and a record's own
creator. Pass the resolved `Capabilities` to the view to hide actions the user
can't perform.

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

## Admin changelist (Django-style list/filter/sort)

Registered entities get a generic, staff-only **changelist** at
`/admin/list/{slug}` — search box, sortable column headers, and pagination —
and their count on `/admin` links to it. This mirrors Django's `ModelAdmin`
changelist: you declare *what* to show, the framework renders it.

To make a model listable, implement the changelist hooks on its `AdminEntity`
(in `fracture-core/src/entity_registry.rs`, or your app's registry):

```rust
fn slug(&self) -> &'static str { "widgets" }              // → /admin/list/widgets

fn columns(&self) -> Vec<AdminColumn> {                    // Django: list_display
    vec![
        AdminColumn::sortable("name", "Name"),            // sortable header
        AdminColumn::plain("status", "Status"),           // display-only
    ]
}

async fn list(&self, db, q: &ListQuery) -> Result<AdminListPage, DbErr> {
    use crate::models::_entities::widgets::{Column, Entity};
    let mut query = Entity::find();
    if let Some(s) = &q.q {                                // Django: search_fields
        query = query.filter(Column::Name.contains(s));
    }
    let dir = if q.desc { Order::Desc } else { Order::Asc };
    query = match q.sort.as_deref() {                      // allow-listed sort columns
        Some("name") => query.order_by(Column::Name, dir),
        _ => query.order_by(Column::Name, Order::Asc),     // Django: ordering (default)
    };
    // `row_fn` projects each model to the row JSON — list ONLY safe fields
    // (never password hashes / api keys), the way `UsersEntity` does.
    paginate_models(db, query, q, self.columns(), |m| serde_json::json!({
        "name": m.name, "status": m.status,
    })).await
}
```

Then `registry.register(Box::new(WidgetsEntity));`. Sorting is allow-listed by
the `match` (only declared columns sort — no SQL injection via `?sort=`), and
`paginate_models` handles the count/offset/limit and page math.

**End users** get one consistent table UX across every model; **developers**
write a policy (`ResourcePolicy`, for per-group permissions) plus this small
changelist declaration — no bespoke list controller or template per model.
