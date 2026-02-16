# Fracture CMS

A content management system built with [Rust](https://www.rust-lang.org/) on the [Loco](https://loco.rs) framework, featuring OIDC single sign-on and movie CRUD management.

## Features

- **OIDC Authentication** — single sign-on through OpenID Connect (Kanidm)
- **Short-lived JWT Sessions** — 15-minute tokens with silent background refresh
- **Logout** — cookie-based session clearing with sign-out link in the nav bar
- **Movie Management** — full CRUD for movie entries with server-side rendered views
- **Content Security Policy** — strict CSP headers with no inline scripts
- **Containerized Development** — Podman Compose environment with Kanidm IdP

## Quick Start

### Local development

```sh
cargo loco start
```

Visit [http://localhost:5150](http://localhost:5150).

### Full stack (with Kanidm, Mailcrab)

```sh
./dev/setup.sh
podman compose up
```

See the `dev/` directory for environment configuration and service setup.

## Architecture

### Authentication Flow

1. User clicks **Sign in** → redirected to Kanidm OIDC provider
2. After successful authentication → callback issues a short-lived JWT (15 min) in an HTTP-only cookie
3. Frontend silently refreshes the token every 12 minutes via `/api/auth/oidc/refresh`
4. On token expiry (inactivity) → "Session expired" message with sign-in link
5. User clicks **Sign out** → cookie cleared, redirected to home

### Security

- HTTP-only, SameSite=Lax cookies (not accessible to JavaScript)
- Strict Content-Security-Policy: `default-src 'none'; script-src 'self'; style-src 'self'`
- X-Content-Type-Options, X-Frame-Options, Referrer-Policy headers
- No inline scripts or event handlers — all JS in external files
