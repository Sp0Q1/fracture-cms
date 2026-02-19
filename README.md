# Fracture CMS

A content management system built with [Rust](https://www.rust-lang.org/) on the [Loco](https://loco.rs) framework, featuring OIDC single sign-on via [Zitadel](https://zitadel.com/) and user-owned movie management.

## Features

- **OIDC Authentication** — single sign-on via OpenID Connect (Zitadel), with automatic account creation and email-based linking
- **Back-channel Logout** — IdP-initiated logout invalidates app sessions (JWKS-verified logout tokens)
- **Short-lived JWT Sessions** — 15-minute tokens with silent background refresh
- **User-owned Movies** — each user manages their own movie collection (enforced via `user_id` foreign key)
- **UUID Public IDs** — all resources use UUIDs in URLs instead of sequential integers
- **Account Menu** — SVG avatar icon with dropdown menu
- **Content Security Policy** — strict CSP headers with no inline scripts or styles
- **Containerized Development** — Podman Compose environment with Zitadel IdP, MailCrab, and SQLite

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) (for local development)
- [Podman](https://podman.io/) and `podman-compose` (for the full stack)

### Local development (app only)

```sh
cp .env.example .env
# Fill in JWT_SECRET, OIDC_CLIENT_ID, OIDC_CLIENT_SECRET, OIDC_PROJECT_ID
cargo loco start
```

Visit [http://localhost:5150](http://localhost:5150).

### Full stack (Zitadel + MailCrab + App)

```sh
./dev/setup.sh            # Provisions Zitadel, creates OIDC app, writes .env
podman compose up -d mailcrab app
```

This starts:

| Service | URL | Purpose |
|---------|-----|---------|
| Zitadel | http://localhost:8080 | Identity provider (OIDC) |
| MailCrab | http://localhost:1080 | Email testing (catches all SMTP) |
| App | http://localhost:5150 | Fracture CMS |

A test user is created automatically — credentials are printed at the end of `setup.sh`.

## Architecture

### Authentication Flow

1. User clicks **Sign in** → redirected to Zitadel OIDC provider
2. After authentication → callback issues a short-lived JWT (15 min) in an HTTP-only cookie
3. Frontend silently refreshes the token every 12 minutes via `/api/auth/oidc/refresh`
4. On token expiry (inactivity) → "Session expired" message with sign-in link
5. User clicks **Sign out** → cookie cleared, redirected to Zitadel end-session endpoint

On first OIDC login, the user account is created automatically. If an account with the same verified email already exists, the OIDC identity is linked to it.

### Back-channel Logout

When a user logs out from Zitadel directly (e.g. from another app in the same IdP), Zitadel POSTs a signed `logout_token` JWT to `/api/auth/oidc/backchannel-logout`. The app:

1. Fetches the IdP's JWKS keys and verifies the token signature
2. Validates `iss`, `aud`, and the `http://schemas.openid.net/event/backchannel-logout` event claim
3. Sets `session_invalidated_at` on the user record
4. Middleware rejects subsequent requests from that user until they re-authenticate

### Movie Ownership

- Movies are scoped to the authenticated user via a `user_id` foreign key
- All movie endpoints require authentication
- Users can only view, edit, and delete their own movies
- Movies are addressed by UUID (`/movies/<uuid>`)

### Security

- HTTP-only, SameSite=Lax cookies (not accessible to JavaScript)
- Strict Content-Security-Policy: `default-src 'none'; script-src 'self'; style-src 'self'`
- X-Content-Type-Options, X-Frame-Options, Referrer-Policy headers
- No inline scripts or event handlers — all JS in external files
- OIDC audience verification (rejects tokens not issued for this app)
- Back-channel logout tokens are signature-verified against the IdP's JWKS

## API Routes

### OIDC Authentication

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/auth/oidc/providers` | List configured OIDC providers |
| GET | `/api/auth/oidc/authorize` | Start OIDC login flow |
| GET | `/api/auth/oidc/callback` | OIDC callback (exchanges code for token) |
| GET | `/api/auth/oidc/logout` | Logout (clears cookie, redirects to IdP) |
| GET | `/api/auth/oidc/refresh` | Refresh JWT token |
| POST | `/api/auth/oidc/backchannel-logout` | Back-channel logout (called by IdP) |

### Movies (requires authentication)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/movies` | List user's movies |
| GET | `/movies/new` | New movie form |
| POST | `/movies` | Create movie |
| GET | `/movies/:pid` | Show movie |
| GET | `/movies/:pid/edit` | Edit movie form |
| POST | `/movies/:pid` | Update movie |
| POST | `/movies/:pid/delete` | Delete movie |

## Development

### Running CI locally

The local CI script mirrors the GitHub Actions pipeline:

```sh
./dev/ci.sh
```

This runs 4 checks inside containers: **rustfmt**, **clippy**, **semgrep**, and **tests**. On first run it builds the CI image (`localhost/fracture-ci`) from `dev/Dockerfile.ci`.

### Project structure

```
src/
  controllers/       # Route handlers
    middleware.rs     # JWT cookie authentication
    movie.rs          # Movie CRUD
    oidc.rs           # OIDC auth + back-channel logout
    oidc_state.rs     # OIDC context and state store
  initializers/
    oidc.rs           # OIDC discovery and client setup
    view_engine.rs    # Tera template engine + i18n
  models/
    _entities/        # SeaORM entity definitions
    movies.rs         # Movie model logic
    users.rs          # User model logic + OIDC account creation
migration/src/        # SeaORM migrations (SQLite)
assets/
  views/              # Tera HTML templates
  static/             # CSS, JS, images
  i18n/               # Fluent locale files (en-US, de-DE)
config/               # Loco YAML config (development, test, production)
dev/                  # Dev tooling (setup script, CI, Dockerfiles)
```

### Configuration

The app is configured via `config/development.yaml` with environment variable overrides. Key variables:

| Variable | Description | Default |
|----------|-------------|---------|
| `JWT_SECRET` | Secret for signing JWT tokens | (required) |
| `OIDC_CLIENT_ID` | OIDC client ID from Zitadel | (required) |
| `OIDC_CLIENT_SECRET` | OIDC client secret | (required) |
| `OIDC_PROJECT_ID` | Zitadel project ID (for audience verification) | (optional) |
| `OIDC_ISSUER_URL` | OIDC issuer URL | `http://localhost:8080` |
| `OIDC_REDIRECT_URI` | OIDC callback URL | `http://localhost:5150/api/auth/oidc/callback` |
| `MAILER_HOST` | SMTP host | `localhost` |
| `DATABASE_URL` | SQLite connection string | `sqlite://fracture-cms_development.sqlite?mode=rwc` |

### Database

Fracture CMS uses **SQLite** for both development and testing. Migrations run automatically on startup (`auto_migrate: true`).

### Tech stack

| Component | Technology |
|-----------|-----------|
| Language | Rust |
| Web framework | [Loco](https://loco.rs) (built on Axum) |
| Database | SQLite via [SeaORM](https://www.sea-ql.org/SeaORM/) |
| Templates | [Tera](https://keats.github.io/tera/) with [Fluent](https://projectfluent.org/) i18n |
| Authentication | OpenID Connect via [openidconnect-rs](https://github.com/ramosbugs/openidconnect-rs) |
| Identity provider | [Zitadel](https://zitadel.com/) (self-hosted) |
| Container runtime | [Podman](https://podman.io/) |
