# Fracture CMS

A multi-tenant content management template built with [Rust](https://www.rust-lang.org/) on the [Loco](https://loco.rs) framework. Features organization-based RBAC, OIDC authentication, and org-scoped data isolation.

The core infrastructure (auth, OIDC, orgs, RBAC, invites) lives in the `fracture-core` library crate. Downstream projects depend on it as a Cargo dependency and only write their own domain code — no forking, no merge conflicts on core updates.

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) (for local development)
- [Podman](https://podman.io/) and `podman-compose` (for the full stack)

### Full stack (recommended)

```sh
./dev/setup.sh            # Provisions Zitadel, creates OIDC app, writes .env
podman compose up -d mailcrab app
```

| Service | URL | Purpose |
|---------|-----|---------|
| App | http://localhost:5150 | Fracture CMS |
| Zitadel | http://localhost:8080 | Identity provider (OIDC) |
| MailCrab | http://localhost:1080 | Email testing |

A test user is created automatically. Credentials are printed at the end of `setup.sh`.

### Local development (app only)

```sh
cp .env.example .env
# Fill in JWT_SECRET, OIDC_CLIENT_ID, OIDC_CLIENT_SECRET, OIDC_PROJECT_ID
cargo loco start
```

## Project Structure

```
fracture-core/                      # Library crate (auth, OIDC, orgs, RBAC, invites)
  src/
    controllers/
      middleware.rs                  # JWT auth, OrgContext, require_user!/require_role! macros
      oidc.rs                        # OIDC login, logout, back-channel logout
      oidc_state.rs                  # OIDC state store (CSRF tokens, PKCE verifiers)
      org.rs                         # Organization CRUD, members, invites, switching
    models/
      _entities/                     # Core SeaORM entities (users, orgs, members, invites)
      users.rs                       # User lookup, OIDC account creation/linking
      organizations.rs               # Org creation, personal orgs, slug lookup
      org_members.rs                 # Membership, OrgRole enum, role hierarchy
      org_invites.rs                 # Email invitations, auto-accept on signup
    initializers/
      oidc.rs                        # OIDC discovery, client setup, JWKS URI
      security_headers.rs            # CSP, X-Frame-Options, etc.
    views/
      org.rs                         # Org view helpers (list, settings, members)
    mailers/
      invite.rs                      # Invitation email (SMTP via background worker)
    lib.rs                           # Module exports + register_templates()
  templates/org/                     # Embedded HTML templates (overridable by app)
  migration/src/                     # Core database migrations

src/                                 # App (domain-specific code only)
  controllers/
    home.rs                          # Dashboard
    project.rs                       # Project CRUD (org-scoped)
    note.rs                          # Note CRUD (project-scoped)
    fallback.rs                      # 404 handler
  models/
    _entities/                       # App entities + re-exports of core entities
    projects.rs                      # Org-scoped project queries
    notes.rs                         # Project-scoped note queries
  views/                             # View helpers (Rust → template context)
  initializers/
    view_engine.rs                   # Tera templates + Fluent i18n + core template registration
  mailers/                           # Re-exports core mailers + app-specific mailers
  app.rs                             # Route registration, hooks

migration/src/                       # App-specific migrations (projects, notes)
assets/
  views/                             # App Tera templates (can override core templates)
  static/                            # CSS, JS, images
  i18n/                              # Fluent locale files (en-US, de-DE)
config/                              # Loco YAML config per environment
docs/                                # Architecture, template guide, resource recipes
dev/
  setup.sh                           # Provisions identity provider + writes .env
  ci.sh                              # Runs all CI checks locally in containers
  Dockerfile.ci                      # CI container image (Rust + SQLite + clippy + rustfmt)
```

## Architecture

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for full details.

### Organizations & RBAC

- **Personal org**: Auto-created on first OIDC login. User is owner.
- **Team orgs**: Created manually. Members invited via email.
- **Four roles**: Owner > Admin > Member > Viewer
- **Org context**: Resolved from `org_pid` cookie on every request
- **Row-level isolation**: All queries scoped by `org_id`

### Authentication

The app delegates all authentication to an OIDC provider:

- **Login**: PKCE authorization code flow with audience verification
- **Sessions**: Short-lived JWT cookies, silently refreshed by the frontend
- **Logout**: Clears cookies and redirects to IdP's end-session endpoint
- **Back-channel logout**: IdP POSTs signed `logout_token`, app verifies and invalidates session
- **Account creation**: First OIDC login creates account + personal org. Pending invites auto-accepted.

### Security

- HTTP-only, SameSite=Lax cookies
- Content-Security-Policy: `default-src 'none'; script-src 'self'; style-src 'self'`
- X-Content-Type-Options, X-Frame-Options, Referrer-Policy headers
- OIDC audience verification + JWKS signature verification

## Routes

### Authentication

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/auth/oidc/authorize` | Start login flow |
| GET | `/api/auth/oidc/callback` | OIDC callback |
| GET | `/api/auth/oidc/logout` | Logout |
| GET | `/api/auth/oidc/refresh` | Refresh JWT |
| POST | `/api/auth/oidc/backchannel-logout` | Back-channel logout |

### Organizations

| Method | Path | Min Role |
|--------|------|----------|
| GET | `/orgs` | (any authed) |
| POST | `/orgs` | (any authed) |
| GET | `/orgs/new` | (any authed) |
| GET | `/orgs/{pid}/settings` | Admin |
| POST | `/orgs/{pid}/settings` | Admin |
| GET | `/orgs/{pid}/members` | Viewer |
| POST | `/orgs/{pid}/members/invite` | Admin |
| POST | `/orgs/{pid}/members/{user_pid}/role` | Admin |
| POST | `/orgs/{pid}/members/{user_pid}/remove` | Admin |
| GET | `/orgs/switch/{pid}` | (member) |
| GET | `/invites/{token}/accept` | (any authed) |

### Projects (org-scoped)

| Method | Path | Min Role |
|--------|------|----------|
| GET | `/projects` | Viewer |
| POST | `/projects` | Member |
| GET | `/projects/new` | Member |
| GET | `/projects/{pid}` | Viewer |
| GET/POST | `/projects/{pid}/edit` | Member |
| DELETE | `/projects/{pid}` | Member |

### Notes (project-scoped)

| Method | Path | Min Role |
|--------|------|----------|
| POST | `/projects/{pid}/notes` | Member |
| GET | `/projects/{pid}/notes/new` | Member |
| GET | `/projects/{pid}/notes/{note_pid}` | Viewer |
| GET/POST | `/projects/{pid}/notes/{note_pid}/edit` | Member |
| DELETE | `/projects/{pid}/notes/{note_pid}` | Member |

## User Flows

### First-Time Sign In
1. User clicks "Sign in" → redirected to OIDC provider
2. After authenticating, the callback creates a user account
3. A **personal organization** is auto-created (user is owner)
4. Any **pending invites** matching the user's email are auto-accepted
5. User lands on the dashboard scoped to their personal org

### Organization Management
- **Create org**: Any user can create team organizations from `/orgs/new`
- **Switch org**: Click the org badge in the nav bar to switch between orgs
- **Org settings**: Admins+ can rename orgs at `/orgs/{pid}/settings`
- **Members**: View members at `/orgs/{pid}/members`, admins+ can invite/remove/change roles

### Invite Flow
1. Admin invites a user by email at `/orgs/{pid}/members`
2. An invite record is created (expires in 7 days) and an **invitation email** is sent via SMTP
3. The invite accept link is also shown on the members page so it can be copied and shared directly
4. If the user already has an account, they accept at `/invites/{token}/accept`
5. If the user doesn't have an account yet, the invite is **auto-accepted** when they sign in via OIDC with the matching email

### Project & Note CRUD
- Members+ can create, edit, and delete projects and notes
- Viewers can only view projects and notes
- All data is scoped to the active organization — switching orgs shows different projects
- Notes are nested under projects: `/projects/{pid}/notes`

### Role Hierarchy
| Role | View | Create/Edit/Delete | Invite Members | Org Settings |
|------|------|-------------------|----------------|--------------|
| Viewer | Yes | No | No | No |
| Member | Yes | Yes | No | No |
| Admin | Yes | Yes | Yes | Yes |
| Owner | Yes | Yes | Yes | Yes |

## Creating a New Project

See [docs/TEMPLATE_GUIDE.md](docs/TEMPLATE_GUIDE.md) for how to create a new project using `fracture-core` as a library dependency.

See [docs/ADDING_RESOURCES.md](docs/ADDING_RESOURCES.md) for a step-by-step recipe for adding new org-scoped resources.

See [docs/UPSTREAM_UPDATES.md](docs/UPSTREAM_UPDATES.md) for updating `fracture-core` in your project.

See [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) for production deployment instructions.

## CI

GitHub Actions runs 4 checks: **rustfmt**, **clippy**, **semgrep**, and **tests**.

To run the same checks locally:

```sh
./dev/ci.sh
```

## Tech Stack

| | |
|---|---|
| Framework | [Loco](https://loco.rs) (Axum) |
| Database | SQLite / [SeaORM](https://www.sea-ql.org/SeaORM/) |
| Templates | [Tera](https://keats.github.io/tera/) + [Fluent](https://projectfluent.org/) i18n |
| Auth | OpenID Connect ([openidconnect-rs](https://github.com/ramosbugs/openidconnect-rs)) |
| CSS | [oat.ink](https://oat.ink) (semantic, zero-dependency) |
| IdP | Any OIDC provider (Zitadel, Keycloak, Auth0, etc.) |
| Runtime | [Podman](https://podman.io/) |
