# Architecture

## Overview

Fracture CMS is a multi-tenant content management system built with Rust, Loco framework, SeaORM, and SQLite. It uses OIDC (OpenID Connect) for authentication and organization-based RBAC for authorization.

## Library / App Split

The project is a Cargo workspace with two main crates:

**`fracture-core`** (library crate) — reusable infrastructure:
- Controllers: `middleware`, `oidc`, `oidc_state`, `org`
- Models + entities: `users`, `organizations`, `org_members`, `org_invites`
- Initializers: `oidc`, `security_headers`
- Views: `base_context()`, `org`
- Mailers: `invite` (with embedded email templates)
- Templates: `org/` HTML templates (embedded via `include_dir!`, overridable by the app)
- Migrations: all core schema (users, orgs, members, invites)
- Macros: `require_user!`, `require_role!`

**App crate** (this project) — domain-specific code:
- Controllers: `home`, `project`, `note`, `fallback`
- Models + entities: `projects`, `notes`
- Views: `home`, `project`, `note`
- Initializer: `view_engine` (registers core templates + i18n)
- Templates: `assets/views/` (app templates + overrides of core templates)
- Migrations: `projects`, `notes`

The app re-exports core modules (e.g., `pub use fracture_core::controllers::{middleware, oidc, org}`) so that everything is accessible under `crate::` paths. Core entities are re-exported through the app's `_entities/mod.rs`, ensuring a single type identity across the codebase.

Core templates are embedded in the `fracture-core` binary. The app's `view_engine` initializer calls `fracture_core::register_templates(tera)`, which only adds templates when no filesystem version exists — placing a file at `assets/views/org/list.html` overrides the embedded version.

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
| uploads        | File uploads (org-scoped)         | belongs_to organizations, users |
| blog_posts     | Blog content (Markdown + HTML)    | belongs_to organizations, users |
| job_definitions | Job type + config + schedule     | belongs_to organizations, has_many job_runs |
| job_runs       | Execution records for jobs        | belongs_to job_definitions, organizations, has_many job_run_diffs |
| job_run_diffs  | Change diffs produced by a run    | belongs_to job_runs |

## Upload Subsystem

The upload subsystem lives in `fracture-core` and provides org-scoped file storage with visibility control.

### Configuration

Read from `settings.uploads` in the Loco YAML config. Falls back to defaults if the key is missing.

| Setting | Default | Description |
|---------|---------|-------------|
| `max_file_size` | 5 MiB | Maximum size per file |
| `max_total_size` | 20 MiB | Maximum total size per request |
| `storage_root` | `/app/data/uploads` | Directory for stored files |
| `allowed_types` | `image/png`, `image/jpeg`, `image/gif`, `image/webp`, `image/svg+xml` | Allowed MIME types |

### Architecture

```
fracture-core/src/upload/
  config.rs       # UploadConfig — deserialized from settings.uploads
  service.rs      # UploadService — validation pipeline, storage, SHA-256 checksum
  mod.rs          # Module exports
```

- **UploadConfig**: Loaded via `UploadConfig::from_settings()`. Apps can extend allowed types in their config YAML.
- **ValidationPipeline**: Checks MIME type against the allow list. Runs before storage.
- **UploadService**: Validates, stores the file on disk (in `storage_root`), computes SHA-256, creates the `uploads` DB record.

### Visibility

Each upload has a `visibility` field: `"org"` (default) or `"public"`.

- **Org**: Requires authenticated user who is a member of the owning org (or a platform admin).
- **Public**: Served to anyone (used for blog images, public assets). Cached with `Cache-Control: public, max-age=86400, immutable`.

Access denied returns 404 (not 403) to prevent enumeration.

### Routes

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| POST | `/api/uploads` | Authenticated | Upload a file (multipart form: `file` + optional `visibility`) |
| GET | `/api/uploads/{pid}` | Depends on visibility | Serve a file |
| DELETE | `/api/uploads/{pid}` | Uploader, org admin, or platform admin | Delete a file |

### Database Schema (`uploads`)

| Column | Type | Description |
|--------|------|-------------|
| `pid` | UUID | Public identifier |
| `org_id` | i32 | Owning organization |
| `uploaded_by` | i32 | User who uploaded |
| `original_name` | String(255) | Original filename |
| `storage_path` | String(512) | Path on disk |
| `content_type` | String(127) | MIME type |
| `size_bytes` | i64 | File size |
| `visibility` | String(16) | `"org"` or `"public"` |
| `checksum_sha256` | String(64) | SHA-256 hex digest |

## Blog System

The blog system lives in `fracture-core`. Blog posts are org-scoped (tied to a "blog org" defined in config) and written in Markdown.

### Configuration

Set `settings.blog.org_slug` in the Loco YAML config to the slug of the organization that owns blog posts. Posts are only served when this is configured.

### Architecture

- **Model (`blog_posts.rs`)**: Markdown body is rendered to HTML via comrak (GFM extensions: tables, strikethrough, autolinks, task lists) in `before_save`. Both `body` (Markdown source) and `body_html` (rendered) are stored.
- **Controller (`blog.rs`)**: Public routes (no auth) for the blog index and post pages. Admin routes (platform admin only) for CRUD and publish/unpublish.
- **Views (`views/blog.rs`)**: Tera view helpers for both public and admin templates.
- **Templates**: `fracture-core/templates/blog/` contains admin templates (list, new, edit). Public templates are provided by the consuming app.

### Routes

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/blog/` | Public | Blog index (published posts) |
| GET | `/blog/{slug}` | Public | Single post by slug |
| GET | `/admin/blog/` | Platform admin | Admin post list (all statuses) |
| GET | `/admin/blog/new` | Platform admin | New post form |
| POST | `/admin/blog/` | Platform admin | Create post |
| GET | `/admin/blog/{pid}/edit` | Platform admin | Edit post form |
| POST | `/admin/blog/{pid}` | Platform admin | Update post |
| POST | `/admin/blog/{pid}/publish` | Platform admin | Set status to "published" |
| POST | `/admin/blog/{pid}/unpublish` | Platform admin | Set status to "draft" |

### Markdown Editor

Blog admin templates use `data-md-editor` on the body textarea. The consuming app is responsible for providing `md-editor.js` in its static assets, which initializes a Markdown editor (toolbar, preview, etc.) for any element with this attribute. fracture-core does not bundle a Markdown editor implementation -- it only provides the data attribute hook.

### Database Schema (`blog_posts`)

| Column | Type | Description |
|--------|------|-------------|
| `pid` | UUID | Public identifier |
| `org_id` | i32 | Owning organization (blog org) |
| `author_id` | i32 | Author user |
| `title` | String | Post title |
| `slug` | String | URL slug (auto-generated from title, unique per org) |
| `body` | Text | Markdown source |
| `body_html` | Text | Rendered HTML (auto-generated in `before_save`) |
| `excerpt` | String? | Optional short summary |
| `status` | String | `"draft"` or `"published"` |
| `published_at` | DateTime? | Set on publish, cleared on unpublish |
| `meta_title` | String? | SEO title override |
| `meta_description` | String? | SEO description |

## Generic Jobs System

The jobs system provides a framework for defining, scheduling, and executing background tasks with diff tracking. fracture-core provides the infrastructure; consuming apps implement `JobExecutor` for their specific job types.

### Architecture

```
fracture-core/src/jobs/
  mod.rs          # JobExecutor trait, JobRegistry, JobResult, JobDiff
```

**`JobExecutor` trait**: Apps implement this to define job behavior:

```rust
#[async_trait]
pub trait JobExecutor: Send + Sync {
    fn job_type(&self) -> &str;
    async fn execute(
        &self,
        db: &DatabaseConnection,
        definition: &job_definitions::Model,
        previous_run: Option<&job_runs::Model>,
    ) -> Result<JobResult, Box<dyn Error + Send + Sync>>;
}
```

**`JobRegistry`**: A global `OnceLock`-backed registry. Apps call `init_job_registry()` at startup with their registered executors. The registry maps `job_type` strings to executor instances.

**`JobResult`**: Returned by executors. Contains a JSON `summary` and a vec of `JobDiff` entries (type, entity key, old/new values).

### Tables

**`job_definitions`** -- what to run:

| Column | Type | Description |
|--------|------|-------------|
| `pid` | UUID | Public identifier |
| `org_id` | i32 | Owning organization |
| `name` | String | Human-readable name |
| `job_type` | String | Maps to a registered `JobExecutor` |
| `schedule` | String? | Cron-like schedule (optional) |
| `enabled` | bool | Whether the job is active |
| `config` | Text (JSON) | Job-specific configuration |

**`job_runs`** -- execution history:

| Column | Type | Description |
|--------|------|-------------|
| `pid` | UUID | Public identifier |
| `job_definition_id` | i32 | Parent definition |
| `org_id` | i32 | Organization |
| `status` | String | `"queued"`, `"running"`, `"completed"`, `"failed"` |
| `started_at` | DateTime? | When execution began |
| `completed_at` | DateTime? | When execution finished |
| `error_message` | Text? | Error details on failure |
| `result_summary` | Text? | JSON summary from `JobResult` |

**`job_run_diffs`** -- changes detected by a run:

| Column | Type | Description |
|--------|------|-------------|
| `job_run_id` | i32 | Parent run |
| `diff_type` | String | Type of change (app-defined) |
| `entity_key` | String | Identifier for the changed entity |
| `old_value` | Text? | Previous state (JSON) |
| `new_value` | Text? | New state (JSON) |

### Routes

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/jobs` | Authenticated | List job definitions for current org |
| GET | `/jobs/{pid}` | Authenticated | Show definition + runs |
| POST | `/jobs/{pid}/run` | Authenticated | Trigger a new queued run |
| GET | `/jobs/{pid}/runs/{run_pid}` | Authenticated | Show a run + its diffs |
| GET | `/admin/jobs` | Platform admin | List all definitions across all orgs |

## Template Overrides

Core templates are embedded in the `fracture-core` binary via `include_dir!`. The app's `view_engine` initializer calls `fracture_core::register_templates(tera)`, which only adds a template when no filesystem version already exists.

To override a core template, place a file at the same relative path under `assets/views/`. For example, `assets/views/org/list.html` overrides the embedded `org/list.html`.

This applies to all core template directories: `org/`, `blog/`, `jobs/`, etc. Apps can override individual templates without forking the core.

## Invite Flow

1. Admin submits invite form on `/orgs/{pid}/members` with email + role
2. `org_invites` row created with 7-day expiry, `pid` (UUID) as the invite token
3. `InviteMailer::send_invite()` sends an email via the background worker (SMTP)
4. Accept link is shown on the members page for the creator to copy/share
5. Existing users accept at `/invites/{token}/accept` → membership created
6. New users: `find_or_create_from_oidc()` calls `find_pending_by_email()` and auto-accepts matching invites on first login

Emails are sent asynchronously via Loco's `MailerWorker` background queue. In development, MailCrab catches all outbound email at `http://localhost:1080`.

## Email (Mailer)

The invite mailer lives in `fracture-core` with its templates embedded via `include_dir!`:

```
fracture-core/src/mailers/
  invite.rs                    # InviteMailer struct
  invite/invite/
    subject.t                  # Email subject template
    html.t                     # HTML body template
    text.t                     # Plain text body template
```

The app re-exports it via `pub use fracture_core::mailers::invite` in `src/mailers/mod.rs`. App-specific mailers are added alongside this re-export.

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
