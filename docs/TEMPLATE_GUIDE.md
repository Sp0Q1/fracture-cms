# Creating a New Project

Fracture CMS uses a library/app architecture. The `fracture-core` crate provides all the core infrastructure (OIDC authentication, organizations, RBAC, invites, email). Your project depends on it as a Cargo dependency and only contains your domain-specific code.

This guide walks through creating a new project from scratch using `fracture-core`.

## 1. Scaffold a Loco App

```bash
cargo install loco
loco new my-project
cd my-project
```

Choose the SaaS (server-rendered) template when prompted.

## 2. Add fracture-core

Add the library crate as a git dependency in your `Cargo.toml`:

```toml
[workspace]
members = [".", "migration"]

[workspace.dependencies]
loco-rs = { version = "0.16" }
sea-orm = { version = "1.1", features = [
  "sqlx-sqlite",
  "runtime-tokio-rustls",
  "macros",
] }

[dependencies]
fracture-core = { git = "https://your-repo/fracture-core.git" }
# ... other deps
```

Add fracture-core's migration crate to your `migration/Cargo.toml`:

```toml
[dependencies]
fracture-core-migration = { git = "https://your-repo/fracture-core.git" }
```

## 3. Wire Up Migrations

Your `migration/src/lib.rs` should compose core migrations with your own:

```rust
pub use sea_orm_migration::prelude::*;

mod m20260301_000001_create_your_table;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        // Core migrations run first (users, orgs, members, invites)
        fracture_core_migration::Migrator::migrations()
            .into_iter()
            .chain(vec![
                // Your app-specific migrations
                Box::new(m20260301_000001_create_your_table::Migration)
                    as Box<dyn MigrationTrait>,
            ])
            .collect()
    }
}
```

## 4. Re-export Core Modules

The app re-exports fracture-core modules so that everything is accessible under `crate::`. This is important because controllers, views, and models all reference each other via `crate::` paths.

### `src/lib.rs`

```rust
pub mod app;
pub mod controllers;
pub mod initializers;
pub mod mailers;
pub mod models;
pub mod views;

// Re-export RBAC macros so controllers can use crate::require_user!
pub use fracture_core::{require_role, require_user};
```

### `src/controllers/mod.rs`

```rust
pub mod home;        // Your controllers
pub mod my_resource;

// Core controllers
pub use fracture_core::controllers::{middleware, oidc, oidc_state, org};
```

### `src/models/mod.rs`

```rust
pub mod _entities;
pub mod my_resource;  // Your models

// Core models
pub use fracture_core::models::{org_invites, org_members, organizations, users};
```

### `src/models/_entities/mod.rs`

```rust
pub mod prelude;
pub mod my_resource;  // Your entities

// Core entities
pub use fracture_core::models::_entities::{org_invites, org_members, organizations, users};
```

### `src/models/_entities/prelude.rs`

```rust
pub use super::my_resource::Entity as MyResource;

// Core entity prelude
pub use fracture_core::models::_entities::prelude::{OrgInvites, OrgMembers, Organizations, Users};
```

### `src/views/mod.rs`

```rust
pub mod home;
pub mod my_resource;  // Your views

// Core views + base_context helper
pub use fracture_core::views::{base_context, org};
```

### `src/initializers/mod.rs`

```rust
pub mod view_engine;  // Your view engine (must call register_templates)

// Core initializers
pub use fracture_core::initializers::{oidc, security_headers};
```

### `src/mailers/mod.rs`

```rust
// Core mailers
pub use fracture_core::mailers::invite;

// Add your own mailers here
```

## 5. Register Core Templates

Core org templates (list, new, settings, members, invite_accept) are embedded in the `fracture-core` binary via `include_dir!`. You need to register them with Tera in your view engine initializer.

In `src/initializers/view_engine.rs`, call `fracture_core::register_templates(tera)` inside the `post_process` closure:

```rust
engines::TeraView::build()?.post_process(move |tera| {
    // Register i18n if needed
    // tera.register_function("t", FluentLoader::new(arc.clone()));

    // Register core templates (only adds templates not already on the filesystem)
    fracture_core::register_templates(tera)
        .map_err(|e| loco_rs::Error::string(&e.to_string()))?;
    Ok(())
})?
```

The `register_templates` function checks `tera.get_template(path).is_err()` before adding each template, so any template you place in `assets/views/` will take precedence over the embedded version.

## 6. Register Routes and Initializers

In `src/app.rs`:

```rust
async fn initializers(_ctx: &AppContext) -> Result<Vec<Box<dyn Initializer>>> {
    Ok(vec![
        Box::new(initializers::view_engine::ViewEngineInitializer),
        Box::new(initializers::oidc::OidcInitializer),
        Box::new(initializers::security_headers::SecurityHeadersInitializer),
    ])
}

fn routes(_ctx: &AppContext) -> AppRoutes {
    AppRoutes::with_default_routes()
        // Core routes
        .add_route(controllers::org::routes())
        .add_route(controllers::org::invite_routes())
        .add_route(controllers::oidc::routes())
        // Your routes
        .add_route(controllers::home::routes())
        .add_route(controllers::my_resource::routes())
}

async fn truncate(ctx: &AppContext) -> Result<()> {
    // Your tables first (children before parents)
    truncate_table(&ctx.db, my_resource::Entity).await?;
    // Core tables
    truncate_table(&ctx.db, org_invites::Entity).await?;
    truncate_table(&ctx.db, org_members::Entity).await?;
    truncate_table(&ctx.db, organizations::Entity).await?;
    truncate_table(&ctx.db, users::Entity).await?;
    Ok(())
}
```

## 7. Configure the App

Copy these config files from the fracture-cms reference project:

- `config/development.yaml` — OIDC, SMTP, database settings
- `.env.example` — required environment variables
- `dev/setup.sh` — identity provider provisioning (adapt for your IdP)
- `Containerfile` / `compose.yaml` — container setup

Key environment variables your app needs:

```bash
JWT_SECRET=<random-32-byte-base64-string>
OIDC_PROVIDER_NAME=kanidm
OIDC_ISSUER_URL=https://auth.example.com
OIDC_CLIENT_ID=<client-id>
OIDC_CLIENT_SECRET=<client-secret>
OIDC_REDIRECT_URI=http://localhost:5150/api/auth/oidc/callback
OIDC_POST_LOGOUT_REDIRECT_URI=http://localhost:5150
APP_URL=http://localhost:5150
```

## What You Get from fracture-core

Out of the box, your project has:

- **OIDC authentication** — login, logout, token refresh, back-channel logout
- **User management** — auto-created on first OIDC login
- **Organizations** — personal orgs (auto-created) + team orgs
- **RBAC** — four roles: Owner > Admin > Member > Viewer
- **Org invites** — email invitations with 7-day expiry, auto-accept on signup
- **Org context** — resolved from `org_pid` cookie on every request
- **Security headers** — CSP, X-Frame-Options, etc.
- **Org templates** — list, new, settings, members, invite accept (overridable)
- **Database migrations** — users, organizations, org_members, org_invites tables
- **`require_user!` / `require_role!` macros** — for auth and RBAC in controllers

## Overriding Core Templates

Core templates are embedded in the `fracture-core` binary. To override any core template, place a file with the same path in your `assets/views/` directory:

```
assets/views/org/list.html          # Overrides core's org list page
assets/views/org/settings.html      # Overrides core's org settings page
```

The app's filesystem templates always take precedence. The core embedded templates are only used as fallbacks.

## Example: Adding an Org-Scoped Resource

See [ADDING_RESOURCES.md](ADDING_RESOURCES.md) for a step-by-step checklist. The key pattern:

```rust
// In your controller
use crate::{require_role, require_user};
use fracture_core::controllers::middleware::{get_current_user, get_org_context_or_default, OrgRole};

pub async fn list(State(ctx): State<AppContext>, /* ... */) -> Result<Response> {
    let user = require_user!(ctx, cookies);
    let org_ctx = get_org_context_or_default(&ctx.db, &user, &cookies).await?;
    require_role!(org_ctx, OrgRole::Viewer);

    let items = MyResource::find_by_org(&ctx.db, org_ctx.org.id).await?;
    // ...
}
```
