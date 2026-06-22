# Fracture CMS

A multi-tenant web application starter built with [Rust](https://www.rust-lang.org/) on the [Loco](https://loco.rs) framework. Provides OIDC authentication, organization management with role-based access control, email invites, and org-scoped data isolation out of the box. Ships with projects and notes as example resources to show the patterns.

The core infrastructure lives in the `fracture-core` library crate. Downstream projects depend on it as a Cargo dependency and only write their own domain code — no forking, no merge conflicts on core updates.

## What You Get

- **OIDC single sign-on** — Delegates authentication to any OpenID Connect provider (Keycloak, Auth0, Zitadel, etc.). No passwords stored in your database. Uses PKCE authorization code flow.
- **Organizations** — New users join one shared default org (named for the client) on first login. Additional orgs are staff-created; within an org, Admins invite members by email.
- **Role-based access control** — Four roles (Owner > Admin > Member > Viewer) enforced at the controller level via `require_role!` macro. All database queries scoped by `org_id`.
- **Email invites** — Admins invite users by email. Invites expire after 7 days. If the invitee doesn't have an account yet, the invite is auto-accepted when they sign in with a matching email.
- **Session management** — JWT stored in HTTP-only cookies. The frontend refreshes the token every 12 minutes; on failure, the user sees a "session expired" message with a re-login link.
- **Security headers** — Content-Security-Policy (`default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; font-src 'self'; connect-src 'self'; form-action 'self'; base-uri 'self'; frame-ancestors 'none'`), plus X-Content-Type-Options, X-Frame-Options, Referrer-Policy, X-Permitted-Cross-Domain-Policies, Strict-Transport-Security, and Permissions-Policy.
- **Back-channel logout** — The IdP can POST a signed `logout_token` to invalidate a user's session server-side. The app verifies the token signature via JWKS before acting on it.
- **File uploads** — Org-scoped file upload API with configurable size limits, MIME type validation, SHA-256 checksums, and visibility control (`org` or `public`). Files stored on disk under a configurable storage root.
- **Blog system** — Markdown-based blog with GFM support (tables, strikethrough, task lists). Posts tied to a configurable blog org. Public routes (cacheable, with an Atom feed at `/blog/feed.xml`) for readers; admin routes for platform admins with draft preview, publish/unpublish (stable first-publish dates), and delete. Markdown rendered to HTML on save via comrak.
- **Generic jobs system** — Define job types, schedule runs (cron), and track diffs. Apps implement the `JobExecutor` trait and register it via `init_job_registry()`; fracture-core's `JobRunnerInitializer` polls for queued runs, executes them, and persists run history and diffs. Org admins create/enable jobs, members trigger runs, and both org-scoped and platform-admin views show live run status.
- **Contact form with self-hosted captcha** — Public `/contact` form protected by [Altcha](https://altcha.org) (open-source proof-of-work, no third-party service, vendored CSP-safe build). Messages land in a platform-admin inbox at `/admin/contact`. The challenge endpoint (`/captcha/challenge`) and `fracture_core::captcha::verify_payload` are reusable for any other public form.
- **Public site included** — The landing (sales) page, blog, and static marketing pages ship with the framework and live outside your repo: `public_base.html` marketing layout (session-aware CTA, no JS), default `site/landing.html`, and `GET /pages/{slug}` serving plain-HTML fragments from `assets/views/site/pages/`. Override any template by placing a same-named file under `assets/views/`; the authenticated app uses your `base.html`.
- **Markdown editor hook** — Blog admin templates use the `data-md-editor` attribute on textareas. Consuming apps provide their own `md-editor.js` to initialize a Markdown editor (toolbar, preview, etc.) for elements with this attribute. fracture-core does not bundle an editor implementation.
- **i18n** — Fluent-based internationalization with locale files in `assets/i18n/`.

## Quick Start

### Prerequisites

- [Podman](https://podman.io/) and `podman-compose`
- [Rust](https://rustup.rs/) (only needed for local `cargo` development outside containers)

### 1. Clone and set up

```sh
git clone <repo-url> my-project
cd my-project
./dev/setup.sh            # Starts Keycloak (imports tenant + staff realms), writes .env
```

### 2. Start the app

```sh
podman compose up -d mailcrab app
```

### 3. Open the app

| Service | URL | Purpose |
|---------|-----|---------|
| App | http://localhost:5150 | Your application |
| Keycloak | http://localhost:8080 | Identity provider admin console (admin / admin) |
| MailCrab | http://localhost:1080 | Catches all outbound email for testing |

A test user is created automatically by `setup.sh`. Credentials are printed at the end of the script.

### 4. Sign in and explore

1. Open http://localhost:5150 and click **Get Started**
2. Sign in with the test user credentials
3. You land on the dashboard — you are placed in the deployment's default org automatically
4. Go to **Organizations** to view your orgs (creating new orgs is staff-only)
5. An org Admin can invite a colleague (or yourself with a different email) from the **Members** page
6. Check http://localhost:1080 to see the invitation email
7. Create a project, add some notes — all scoped to the active org
8. Switch between orgs using the dropdown in the nav bar

### Local development (without containers)

```sh
cp .env.example .env
# Fill in JWT_SECRET, OIDC_CLIENT_ID, OIDC_CLIENT_SECRET, OIDC_PROJECT_ID
cargo loco start
```

This requires a running OIDC provider and SMTP server configured in `.env`.

## How It Works

### Authentication

The app never handles passwords. All authentication is delegated to an OIDC provider:

1. User clicks "Sign in" and is redirected to the IdP with a PKCE challenge
2. After authenticating, the IdP redirects back with an authorization code
3. The app exchanges the code for an ID token, verifies the signature via JWKS, and checks audience claims
4. A JWT session cookie is set (HTTP-only, SameSite=Lax)
5. On first login, a user record is created and the user joins the deployment's default org. Pending invites matching the email are auto-accepted.

### Organizations & Roles

Every piece of data belongs to an organization. The active org is tracked via an `org_pid` cookie.

| Role | View | Create/Edit/Delete | Invite Members | Org Settings |
|------|------|-------------------|----------------|--------------|
| Viewer | Yes | No | No | No |
| Member | Yes | Yes | No | No |
| Admin | Yes | Yes | Yes | Yes |
| Owner | Yes | Yes | Yes | Yes |

Roles are enforced in every controller via `require_role!(org_ctx, OrgRole::Member)`. All database queries are scoped by `org_id` — there is no code path that returns data across orgs.

### Invite Flow

1. Admin enters an email and role on the members page
2. An invite record is created (expires in 7 days) and an email is sent via SMTP
3. The accept link is also shown on the page so it can be copied directly
4. Existing users click the link to join. New users are auto-added when they first sign in with a matching email.

## Project Structure

```
fracture-core/                      # Library crate (reusable across projects)
  src/
    controllers/
      middleware.rs                  # JWT auth, OrgContext, require_user!/require_role! macros
      oidc.rs                        # OIDC login, logout, back-channel logout
      oidc_state.rs                  # OIDC state store (CSRF tokens, PKCE verifiers)
      org.rs                         # Organization CRUD, members, invites, switching
      uploads.rs                     # File upload API (create, serve, delete)
      blog.rs                        # Blog public + admin routes
      jobs.rs                        # Job definitions, runs, diffs, trigger
    models/
      _entities/                     # Core SeaORM entities
      users.rs                       # User lookup, OIDC account creation/linking
      organizations.rs               # Org creation, default-org join, slug lookup
      org_members.rs                 # Membership, OrgRole enum, role hierarchy
      org_invites.rs                 # Email invitations, auto-accept on signup
      uploads.rs                     # Upload queries, Visibility enum
      blog_posts.rs                  # Blog queries, Markdown rendering
      job_definitions.rs             # Job definition queries
      job_runs.rs                    # Job run lifecycle
      job_run_diffs.rs               # Diff queries
    upload/
      config.rs                      # UploadConfig (size limits, allowed types, storage root)
      service.rs                     # UploadService (validation, storage, checksums)
    jobs/
      mod.rs                         # JobExecutor trait, JobRegistry, JobResult, JobDiff
    initializers/
      oidc.rs                        # OIDC discovery, client setup, JWKS URI
      security_headers.rs            # CSP, X-Frame-Options, etc.
    views/
      org.rs                         # Org view helpers (list, settings, members)
      blog.rs                        # Blog view helpers (public + admin)
      jobs.rs                        # Jobs view helpers (org + admin)
    mailers/
      invite.rs                      # Invitation email (SMTP via background worker)
    lib.rs                           # Module exports + register_templates()
  templates/org/                     # Embedded org templates (overridable by app)
  templates/blog/                    # Embedded blog admin templates (overridable by app)
  static/upload.js                   # Upload helper script
  migration/src/                     # Core database migrations

src/                                 # App (your domain-specific code)
  controllers/
    home.rs                          # Dashboard
    project.rs                       # Project CRUD (org-scoped) — example resource
    note.rs                          # Note CRUD (project-scoped) — example resource
    fallback.rs                      # 404 handler
  models/
    _entities/                       # App entities + re-exports of core entities
    projects.rs                      # Org-scoped project queries
    notes.rs                         # Project-scoped note queries
  views/                             # View helpers (Rust -> template context)
  initializers/
    view_engine.rs                   # Tera templates + Fluent i18n + core template registration
  mailers/                           # Re-exports core mailers + app-specific mailers
  app.rs                             # Route registration, hooks

migration/src/                       # App-specific migrations (projects, notes)
assets/
  views/                             # App Tera templates (can override core templates)
  static/                            # CSS, JS, images
  i18n/                              # Fluent locale files (en-US, de-DE)
fracture-ctl/                        # CLI tool for deployment management
  src/main.rs                        # init, up, down, backup, restore, admin, ci, dev, update
config/                              # Loco YAML config per environment
docs/                                # Architecture, template guide, resource recipes
dev/
  setup.sh                           # Starts Keycloak + writes .env
  ci.sh                              # Runs all CI checks locally in containers
  Dockerfile.ci                      # CI container image (Rust + SQLite + clippy + rustfmt)
```

## Routes

### Authentication

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/auth/oidc/authorize` | Start OIDC login flow |
| GET | `/api/auth/oidc/callback` | OIDC callback (exchanges code for token) |
| GET | `/api/auth/oidc/logout` | Clear session and redirect to IdP logout |
| GET | `/api/auth/oidc/refresh` | Refresh JWT cookie |
| POST | `/api/auth/oidc/backchannel-logout` | IdP-initiated session invalidation |

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

### Projects (org-scoped example)

| Method | Path | Min Role |
|--------|------|----------|
| GET | `/projects` | Viewer |
| POST | `/projects` | Member |
| GET | `/projects/new` | Member |
| GET | `/projects/{pid}` | Viewer |
| GET | `/projects/{pid}/edit` | Member |
| POST | `/projects/{pid}` | Member |
| DELETE | `/projects/{pid}` | Member |

### Notes (project-scoped example)

| Method | Path | Min Role |
|--------|------|----------|
| POST | `/projects/{pid}/notes` | Member |
| GET | `/projects/{pid}/notes/new` | Member |
| GET | `/projects/{pid}/notes/{note_pid}` | Viewer |
| GET | `/projects/{pid}/notes/{note_pid}/edit` | Member |
| POST | `/projects/{pid}/notes/{note_pid}` | Member |
| DELETE | `/projects/{pid}/notes/{note_pid}` | Member |

### Uploads

| Method | Path | Auth |
|--------|------|------|
| POST | `/api/uploads` | Authenticated |
| GET | `/api/uploads/{pid}` | Public or org member |
| DELETE | `/api/uploads/{pid}` | Uploader / org admin |

### Blog

| Method | Path | Auth |
|--------|------|------|
| GET | `/blog/` | Public |
| GET | `/blog/feed.xml` | Public |
| GET | `/pages/{slug}` | Public |
| GET/POST | `/contact` | Public (Altcha-gated POST) |
| GET | `/captcha/challenge` | Public |
| GET/POST | `/admin/contact/...` | Platform admin |
| GET | `/blog/{slug}` | Public |
| GET/POST | `/admin/blog/...` | Platform admin |

### Jobs

| Method | Path | Auth |
|--------|------|------|
| GET | `/jobs` | Authenticated |
| POST | `/jobs` | Org admin |
| GET | `/jobs/{pid}` | Authenticated |
| POST | `/jobs/{pid}/toggle` | Org admin |
| POST | `/jobs/{pid}/run` | Org member |
| GET | `/jobs/{pid}/runs/{run_pid}` | Authenticated |
| GET | `/admin/jobs` | Platform admin |

## fracture-ctl

CLI tool for managing deployments. Install from [GitHub Releases](https://github.com/Sp0Q1/fracture-cms/releases). See [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) for the full walkthrough.

| Command | Description |
|---------|-------------|
| `init --image <img> [--repo <url>]` | Generate production config (`.env` + `compose.prod.yaml`) |
| `up` | Pull latest image, auto-backup, start services |
| `down` | Stop all services |
| `backup [-o file]` | Back up the database |
| `restore <file> [--yes]` | Restore from backup |
| `admin set <email>` | Promote user to platform admin |
| `admin list` | List platform admins |
| `ci` | Run CI checks via `dev/ci.sh` |
| `dev [--setup]` | Start the dev stack |
| `update` | Self-update to latest release |

## Building on This

See [docs/TEMPLATE_GUIDE.md](docs/TEMPLATE_GUIDE.md) for how to create a new project using `fracture-core` as a library dependency.

See [docs/ADDING_RESOURCES.md](docs/ADDING_RESOURCES.md) for a step-by-step recipe for adding new org-scoped resources (replace projects/notes with your domain).

See [docs/UPSTREAM_UPDATES.md](docs/UPSTREAM_UPDATES.md) for updating `fracture-core` in your project.

See [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) for production deployment instructions.

See [docs/FEDERATION.md](docs/FEDERATION.md) for running many tenant deployments on separate servers/domains with a shared identity provider — central IAM with hard trust boundaries between tenants.

## CI

GitHub Actions runs 4 checks: **rustfmt**, **clippy** (with pedantic lints), **semgrep**, and **tests**.

To run the same checks locally:

```sh
./dev/ci.sh
```

## Tech Stack

| | |
|---|---|
| Language | [Rust](https://www.rust-lang.org/) |
| Framework | [Loco](https://loco.rs) (built on [Axum](https://github.com/tokio-rs/axum)) |
| Database | SQLite via [SeaORM](https://www.sea-ql.org/SeaORM/) (PostgreSQL also supported) |
| Templates | [Tera](https://keats.github.io/tera/) + [Fluent](https://projectfluent.org/) i18n |
| Auth | OpenID Connect via [openidconnect-rs](https://github.com/ramosbugs/openidconnect-rs) |
| CSS | [oat.ink](https://oat.ink) (semantic HTML styling, no build step) |
| IdP | Any OIDC provider (Keycloak ships in the dev stack; Auth0, Zitadel, etc. also work) |
| Containers | [Podman](https://podman.io/) |
