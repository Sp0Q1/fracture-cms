# Architecture

## Overview

Fracture CMS is a multi-tenant content management system built with Rust, Loco framework, SeaORM, and SQLite. It uses OIDC (OpenID Connect) for authentication and organization-based RBAC for authorization.

## Authentication Flow

1. User clicks "Sign in" → redirected to OIDC provider
2. PKCE + CSRF tokens stored server-side with 5-minute TTL
3. Provider redirects back with authorization code
4. Server exchanges code for ID token, verifies JWT signature against JWKS
5. `find_or_create_from_oidc()` either finds existing user or creates new one
6. On new user creation: personal org created, pending invites auto-accepted
7. JWT session cookie set (HTTP-only, SameSite=Lax)
8. `org_pid` cookie set to user's first org

## Organization Model

- **Personal org**: Auto-created on first OIDC login. Cannot be deleted. User is owner.
- **Team orgs**: Created manually. Users can be invited via email.
- **Org context**: Resolved on every request from `org_pid` cookie → falls back to first org.

## RBAC (Role-Based Access Control)

Four roles with strict hierarchy:

```
Owner > Admin > Member > Viewer
```

| Role    | View | Create/Edit/Delete | Manage Members | Org Settings |
|---------|------|-------------------|----------------|--------------|
| Viewer  | Yes  | No                | No             | No           |
| Member  | Yes  | Yes               | No             | No           |
| Admin   | Yes  | Yes               | Yes            | Yes          |
| Owner   | Yes  | Yes               | Yes            | Yes          |

Implemented via `OrgRole` enum with `PartialOrd` and `at_least()` method.

## Data Access Patterns

All org-scoped tables include an `org_id` column. Every query helper is scoped by `org_id`:

```rust
// Example: find projects scoped to an org
Entity::find()
    .filter(Column::OrgId.eq(org_id))
    .all(db)
```

Cross-org data access is impossible through the standard query helpers.

## Request Lifecycle

```
Request → get_current_user(jwt cookie)
        → get_org_context_or_default(org_pid cookie)
        → require_role!(org_ctx, minimum_role)
        → controller logic (scoped by org_ctx.org.id)
        → view rendering (with base_context)
        → response
```

## Key Tables

| Table          | Purpose                           | Key Relations              |
|----------------|-----------------------------------|---------------------------|
| users          | User accounts                     | has_many org_members      |
| organizations  | Orgs (personal + team)            | has_many org_members, projects, notes |
| org_members    | User-org membership + role        | belongs_to users, organizations |
| org_invites    | Email-based invitations           | belongs_to organizations, users |
| projects       | Org-scoped projects               | belongs_to organizations, has_many notes |
| notes          | Project-scoped notes              | belongs_to projects, organizations |

## Invite Flow

1. Admin submits invite form on `/orgs/{pid}/members` with email + role
2. `org_invites` row created with 7-day expiry, `pid` (UUID) as the invite token
3. `InviteMailer::send_invite()` sends an email via the background worker (SMTP)
4. Accept link is shown on the members page for the creator to copy/share
5. Existing users accept at `/invites/{token}/accept` → membership created
6. New users: `find_or_create_from_oidc()` calls `find_pending_by_email()` and auto-accepts matching invites on first login

Emails are sent asynchronously via Loco's `MailerWorker` background queue. In development, MailCrab catches all outbound email at `http://localhost:1080`.

## Email (Mailer)

Mailers live in `src/mailers/` with Tera templates in subdirectories:

```
src/mailers/
  invite.rs                    # InviteMailer struct
  invite/invite/
    subject.t                  # Email subject template
    html.t                     # HTML body template
    text.t                     # Plain text body template
```

Emails are enqueued as background jobs via `Mailer::mail_template()` and processed by `MailerWorker`. SMTP is configured in `config/*.yaml` under the `mailer.smtp` key.

## Frontend Conventions

- **CSS framework**: [oat.ink](https://oat.ink) — semantic HTML styling with no classes needed for basic elements
- **No inline CSS**: Use oat.ink utility classes (`.mt-4`, `.mb-6`, `.hstack`, `.vstack`, etc.) or `app.css`
- **No inline JavaScript**: All behavior uses `data-` attributes handled by `app.js`
  - `data-href` — clickable rows/cards
  - `data-delete-url` + `data-delete-redirect` — delete confirmation
  - `data-copy` — copy to clipboard
  - `data-select-on-focus` — select input text on focus
  - `data-submit-on-change` — auto-submit form on select change
- **CSP enforced**: `script-src 'self'; style-src 'self'` — no `unsafe-inline` or `unsafe-eval`
- **Tera auto-escaping**: `.html` templates auto-escape by default — do not use `| escape` filters (causes double-escaping)

## Public IDs

All entities use `pid` (UUID v4) as the public-facing identifier. Internal `id` (i32 auto-increment) is never exposed in URLs or API responses. `pid` is generated in `before_save()` on insert.
