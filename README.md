# Fracture CMS

A content management system built with [Rust](https://www.rust-lang.org/) on the [Loco](https://loco.rs) framework. Users authenticate via OpenID Connect (Zitadel) and manage their own movie collections.

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
| Zitadel | http://localhost:8080 | Identity provider |
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
src/
  controllers/
    middleware.rs       # JWT cookie authentication
    movie.rs            # Movie CRUD (list, create, show, edit, delete)
    oidc.rs             # OIDC login, logout, back-channel logout
    oidc_state.rs       # OIDC state store (CSRF tokens, PKCE verifiers)
  initializers/
    oidc.rs             # OIDC discovery, client setup, JWKS URI extraction
    view_engine.rs      # Tera templates + Fluent i18n
  models/
    _entities/          # SeaORM entity definitions (hand-edited)
    movies.rs           # Movie queries scoped to user
    users.rs            # User lookup, OIDC account creation/linking
migration/src/          # Database migrations (SQLite)
assets/
  views/                # Tera HTML templates
  static/               # CSS, JS, images
  i18n/                 # Fluent locale files (en-US, de-DE)
config/                 # Loco YAML config per environment
dev/
  setup.sh              # Provisions Zitadel + writes .env
  ci.sh                 # Runs all CI checks locally in containers
  Dockerfile.ci         # CI container image (Rust + SQLite + clippy + rustfmt)
```

## Architecture

### Authentication

The app delegates all authentication to [Zitadel](https://zitadel.com/) via OpenID Connect:

- **Login**: PKCE authorization code flow with audience verification
- **Sessions**: Short-lived JWT cookies (15 min), silently refreshed by the frontend
- **Logout**: Clears the cookie and redirects to Zitadel's end-session endpoint
- **Back-channel logout**: When a user logs out from Zitadel directly, the IdP POSTs a signed `logout_token`. The app verifies the signature against the IdP's JWKS, then sets `session_invalidated_at` on the user. Middleware rejects requests until the user re-authenticates.
- **Account creation**: First OIDC login creates an account automatically. If an account with the same verified email exists, the OIDC identity is linked to it.

### Data Model

- **Users** have OIDC identity fields (`oidc_provider`, `oidc_subject`) and a `session_invalidated_at` timestamp for back-channel logout
- **Movies** belong to a user via `user_id` foreign key. All queries are scoped to the authenticated user
- Both entities use UUIDs (`pid`) as public-facing identifiers; internal `id` (integer) is never exposed

### Security

- HTTP-only, SameSite=Lax cookies
- Content-Security-Policy: `default-src 'none'; script-src 'self'; style-src 'self'`
- X-Content-Type-Options, X-Frame-Options, Referrer-Policy headers
- OIDC audience verification
- JWKS signature verification on back-channel logout tokens

## Routes

### Authentication

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/auth/oidc/providers` | List configured providers |
| GET | `/api/auth/oidc/authorize` | Start login flow |
| GET | `/api/auth/oidc/callback` | OIDC callback |
| GET | `/api/auth/oidc/logout` | Logout |
| GET | `/api/auth/oidc/refresh` | Refresh JWT |
| POST | `/api/auth/oidc/backchannel-logout` | Back-channel logout (called by IdP) |

### Movies (authenticated)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/movies` | List |
| GET | `/movies/new` | New form |
| POST | `/movies` | Create |
| GET | `/movies/:pid` | Show |
| GET | `/movies/:pid/edit` | Edit form |
| POST | `/movies/:pid` | Update |
| POST | `/movies/:pid/delete` | Delete |

## Configuration

Configured via `config/<environment>.yaml` with environment variable overrides:

| Variable | Description | Default |
|----------|-------------|---------|
| `JWT_SECRET` | JWT signing secret | (required) |
| `OIDC_CLIENT_ID` | OIDC client ID | (required) |
| `OIDC_CLIENT_SECRET` | OIDC client secret | (required) |
| `OIDC_PROJECT_ID` | Zitadel project ID for audience verification | (optional) |
| `OIDC_ISSUER_URL` | OIDC issuer URL | `http://localhost:8080` |
| `OIDC_REDIRECT_URI` | OIDC callback URL | `http://localhost:5150/api/auth/oidc/callback` |
| `DATABASE_URL` | SQLite connection string | `sqlite://fracture-cms_development.sqlite?mode=rwc` |
| `MAILER_HOST` | SMTP host | `localhost` |

## CI

GitHub Actions runs 4 checks: **rustfmt**, **clippy**, **semgrep**, and **tests**.

To run the same checks locally:

```sh
./dev/ci.sh
```

This uses a pre-built container image (`dev/Dockerfile.ci`) so no local Rust toolchain is required.

## Tech Stack

| | |
|---|---|
| Framework | [Loco](https://loco.rs) (Axum) |
| Database | SQLite / [SeaORM](https://www.sea-ql.org/SeaORM/) |
| Templates | [Tera](https://keats.github.io/tera/) + [Fluent](https://projectfluent.org/) i18n |
| Auth | OpenID Connect ([openidconnect-rs](https://github.com/ramosbugs/openidconnect-rs)) |
| IdP | [Zitadel](https://zitadel.com/) (self-hosted) |
| Runtime | [Podman](https://podman.io/) |
