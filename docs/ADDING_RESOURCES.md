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
```

Register in `src/models/mod.rs`.

## 4. Controller

Create `src/controllers/<resource>.rs`:

- Use `require_user!` macro for authentication
- Use `get_org_context_or_default()` for org resolution
- Use `require_role!` macro for authorization
- Scope all queries by `org_ctx.org.id`
- Define `routes()` function

Register in `src/controllers/mod.rs` and `src/app.rs`.

## 5. View

Create `src/views/<resource>.rs`:

- Accept `user`, `org_ctx`, `user_orgs` params
- Use `super::base_context()` for common template vars
- Add resource-specific data to context

Register in `src/views/mod.rs`.

## 6. Templates

Create `assets/views/<resource>/`:

- `list.html` — Table/grid with items, role-gated "New" button
- `create.html` — Form, breadcrumb
- `show.html` — Detail view, role-gated edit/delete buttons
- `edit.html` — Edit form, role-gated delete button

All templates extend `base.html`.

## 7. Route Registration

In `src/app.rs`:

```rust
.add_route(controllers::<resource>::routes())
```

## 8. Tests

Create `tests/models/<resource>.rs`:

- Test CRUD operations scoped by org_id
- Test cross-org isolation
- Test pid generation on insert

Register in `tests/models/mod.rs`.

## 9. Truncation Order

Update `truncate()` in `src/app.rs` — child tables before parent tables (foreign key order).
