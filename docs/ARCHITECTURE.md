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

## Public IDs

All entities use `pid` (UUID v4) as the public-facing identifier. Internal `id` (i32 auto-increment) is never exposed in URLs or API responses. `pid` is generated in `before_save()` on insert.
