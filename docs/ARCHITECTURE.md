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
6. On new user creation: **no personal org is created**. After the user record
   exists, the OIDC callback places the user in the deployment's single default
   org (configured via `settings.org.default_slug` / `default_name`, created on
   first use) at `Viewer` role. Pending invites are auto-accepted **only when
   the IdP asserted `email_verified`** (or the operator set
   `assume_email_verified` for IdPs that omit the claim). Linking OIDC to an
   existing email-matched account requires the same assertion — an unverified
   email is refused to prevent account takeover.
7. JWT session cookie set (HTTP-only, SameSite=Lax)
8. `org_pid` cookie set to user's first org

## Organization Model

- **Default org**: One shared org per deployment (named for the client),
  configured via `settings.org.default_slug` / `default_name`. New users join it
  on first login at the role set by `settings.org.default_role` (defaults to
  `member`); it is created on first use if missing. There are **no per-user
  personal orgs**. Leave the slug empty to disable the auto-join.
- **Additional orgs**: Staff-only. Creating and configuring orgs requires
  platform-admin; clients request additional orgs out of band. Within an org,
  client Admins/Owners manage their own members and settings.
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

Membership writes enforce a **role ceiling at the model layer**:
`org_members::Model::update_role` / `remove_member` take the actor's role and
refuse (as `MemberWriteError::Forbidden`) any change where the actor does not
outrank-or-equal both the target's current role and the granted role. The
check runs inside the write transaction against a freshly fetched (and, on
PostgreSQL, row-locked) membership, so a concurrent promotion of the target
cannot slip past a stale controller-side check. The same transaction guards
against demoting or removing an org's last owner.

## Data Access Patterns

All org-scoped tables include an `org_id` column. Every query helper is scoped by `org_id`:

```rust
// Example: find projects scoped to an org
Entity::find()
    .filter(Column::OrgId.eq(org_id))
    .all(db)
```

Cross-org data access is impossible through the standard query helpers.

### `OrgScoped` trait — pinning the contract at the type level

The `models::OrgScoped` trait declares that an entity has an `org_id` column. The blanket `OrgScopedQuery` impl gives those entities a safe `find_in_org(org_id)` helper. This makes "every query is org-scoped" the easy path:

```rust
use fracture_core::models::OrgScopedQuery;

let posts = blog_posts::Entity::find_in_org(org_id).all(&db).await?;
```

Implementations live in each entity's domain module (e.g. `models/blog_posts.rs`). Implemented today on: `blog_posts`, `job_definitions`, `job_runs`, `org_invites`, `org_members`, `uploads`, `resource_assignments`. Add an impl for every new org-owned entity.

### Generic per-resource role grants — `ResourceAssignment`

Per-resource role grants (e.g. "this user is a pentester *on this engagement*") go through the `resource_assignments` table. fracture-core provides the **mechanism**; downstream crates own the **semantics** of each `role_key`:

```rust
use fracture_core::models::resource_assignments::{Model as ResourceAssignment, AssignParams};

ResourceAssignment::assign(&db, AssignParams {
    user_id: pentester.id,
    org_id: customer_org.id,
    resource_type: "engagement",   // PT-domain string
    resource_id: engagement.id,
    role_key: "pentester",         // PT-domain string
    granted_by: Some(admin.id),
    expires_at: Some(deadline),
}).await?;

// Authorization check — the canonical helper:
if ResourceAssignment::has_assignment(&db, user.id, "engagement", id, "pentester").await? {
    // proceed
}
```

Active = `revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now())`.

Downstream crates **must not** introduce parallel assignment tables; this is the single sanctioned mechanism. Adding a new role identifier (e.g. `"reviewer"`) is a one-line domain change in the consuming crate, not a CMS change.

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
| organizations  | Orgs (default + staff-created)     | has_many org_members, projects, notes |
| org_members    | User-org membership + role        | belongs_to users, organizations |
| org_invites    | Email-based invitations           | belongs_to organizations, users |
| projects       | Org-scoped projects               | belongs_to organizations, has_many notes |
| notes          | Project-scoped notes              | belongs_to projects, organizations |
| uploads        | File uploads (org-scoped)         | belongs_to organizations, users |
| blog_posts     | Blog content (Markdown + HTML)    | belongs_to organizations, users |
| job_definitions | Job type + config + schedule     | belongs_to organizations, has_many job_runs |
| job_runs       | Execution records for jobs        | belongs_to job_definitions, organizations, has_many job_run_diffs |
| job_run_diffs  | Change diffs produced by a run    | belongs_to job_runs |
| resource_assignments | Per-resource role grants    | belongs_to users, organizations |

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

- **Model (`blog_posts.rs`)**: Markdown body is rendered to HTML via comrak (GFM extensions: tables, strikethrough, autolinks, task lists) in `before_save`, with `render.unsafe = false` so raw HTML in the source never reaches the page. Both `body` (Markdown source) and `body_html` (rendered) are stored. Admin mutations resolve posts via `find_by_pid_and_org` against the blog org.
- **Controller (`blog.rs`)**: Public routes (no auth, marked `Cache-Control: public` — they carry no session state) for the blog index, posts, and the Atom feed. Admin routes (platform admin only) for CRUD, publish/unpublish, draft preview, and delete. `published_at` is the *first* publication date: unpublish keeps it and republish does not re-stamp it, so the blog and feed order stay stable.
- **Views (`views/blog.rs`)**: Tera view helpers plus the Atom feed builder (hand-escaped XML).
- **Templates**: `fracture-core/templates/blog/` contains both the public templates (which extend the public layout, see below) and the admin templates (which extend the app layout). All are overridable by the consuming app.

### Public vs. app surface

The project is a B2B application first: the authenticated product renders with `base.html` (org switcher, account menu, session refresh). The supporting public surface — landing page, blog, static pages — is owned by fracture-core so downstream repos carry **zero files** for it, and every piece is template-overridable:

- **`public_base.html`** — the marketing layout: no org chrome, no JavaScript. Its only session awareness is the nav CTA (Dashboard when signed in, Sign in otherwise), driven by `user_name` in the context; handlers set `Cache-Control: public` only on the guest variant, which is identical for every visitor.
- **`site/landing.html`** — the default sales page, served to guests at `/` via `views::site::landing()` (the app's home controller delegates its guest branch). Override it with your own design.
- **Static pages** — `GET /pages/{slug}` renders `site/pages/{slug}.html` wrapped in the public layout via `site/page_frame.html`. Fragments are plain HTML with **no `extends`** — that is deliberate: the app's Tera loads `assets/views` before core templates register (and re-loads on hot reload), so app-side templates cannot extend core-embedded layouts. The frame wraps the rendered fragment instead, so adding a marketing page is literally dropping one file into `assets/views/site/pages/`. Unknown/invalid slugs 404.

To replace the whole public look, override `public_base.html` itself (app-side overrides of the *layout* are standalone files, so they have no inheritance constraint).

### Routes

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/blog/` | Public | Blog index (published posts, cacheable) |
| GET | `/blog/feed.xml` | Public | Atom feed of published posts |
| GET | `/blog/{slug}` | Public | Single post by slug |
| GET | `/admin/blog/` | Platform admin | Admin post list (all statuses) |
| GET | `/admin/blog/new` | Platform admin | New post form |
| POST | `/admin/blog/` | Platform admin | Create post |
| GET | `/admin/blog/{pid}/edit` | Platform admin | Edit post form |
| POST | `/admin/blog/{pid}` | Platform admin | Update post |
| POST | `/admin/blog/{pid}/publish` | Platform admin | Publish (first publish stamps `published_at`) |
| POST | `/admin/blog/{pid}/unpublish` | Platform admin | Set status to "draft" (keeps `published_at`) |
| GET | `/admin/blog/{pid}/preview` | Platform admin | Render any status with the public template |
| POST | `/admin/blog/{pid}/delete` | Platform admin | Permanently delete |

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

The jobs system provides a framework for defining, scheduling, and executing background tasks with diff tracking. fracture-core provides the infrastructure — including the runner that actually executes queued runs; consuming apps implement `JobExecutor` for their specific job types and wire two startup hooks (see "Wiring" below).

### Architecture

```
fracture-core/src/jobs/
  mod.rs          # JobExecutor trait, JobRegistry, JobResult, JobDiff
  runner.rs       # JobRunnerInitializer + the execution/scheduling loop
```

**`JobExecutor` trait**: Apps implement this to define job behavior:

```rust
#[async_trait]
pub trait JobExecutor: Send + Sync {
    fn job_type(&self) -> &str;
    fn label(&self) -> &str { self.job_type() }       // friendly name in the picker
    fn description(&self) -> &'static str { "" }       // one-line picker blurb
    // Per-job config form, built per-request so options can be dynamic
    // (e.g. a dropdown of the org's projects). Empty = no custom form.
    async fn config_form(&self, db: &DatabaseConnection, org_id: i32)
        -> Result<Vec<FormField>, DbErr> { Ok(Vec::new()) }
    async fn execute(
        &self,
        db: &DatabaseConnection,
        definition: &job_definitions::Model,
        previous_run: Option<&job_runs::Model>,
    ) -> Result<JobResult, Box<dyn Error + Send + Sync>>;
}
```

**Friendly creation (no JSON/cron for end users).** A job type declares its
config inputs via `config_form`, and the create flow renders them instead of a
raw config textarea: `/jobs` shows a picker of registered types (label +
description), and `/jobs/new/{job_type}` is that type's form. Submitted field
values are collected into the definition's `config` JSON under each field's
`name`, which `execute` reads back; the schedule is a friendly preset selector
(Manual / Hourly / Daily / …) over cron expressions. A type that declares no
`config_form` falls back to a raw JSON `config` field, so advanced or
container-style jobs aren't constrained by a fixed UI. The reference app's
`write_note` job is the worked example (a project dropdown + an optional
title); `content_stats` declares no form and needs no config.

**`JobRegistry`**: A global `OnceLock`-backed registry. Apps call `init_job_registry()` at startup with their registered executors. The registry maps `job_type` strings to executor instances.

**`JobResult`**: Returned by executors. Contains a JSON `summary` and a vec of `JobDiff` entries (type, entity key, old/new values).

### Execution lifecycle (the runner)

`jobs::runner::JobRunnerInitializer` spawns a polling loop (default every 15s, configurable). Each tick:

1. **Scheduling** — every enabled definition with a `schedule` (cron expression *with a seconds field*, e.g. `0 0 * * * *` for hourly) gets a run enqueued when an occurrence has passed since its last run. A never-run scheduled definition is due immediately. A definition with a queued/running run is skipped, so slow jobs can't pile up a backlog.
2. **Execution** — queued runs are claimed oldest-first with an atomic compare-and-swap (`UPDATE … WHERE status = 'queued'`), transitioning them `queued → running` (stamping `started_at`). The matching `JobExecutor` runs with the definition and the previous *completed* run; on success the run is marked `completed` with `result_summary` and its diffs persisted to `job_run_diffs`; on error it is marked `failed` with `error_message`. Runs whose definition was deleted, disabled, or has no registered executor fail with a descriptive error instead of executing (or sitting queued forever).

Executors must return `Err` on failure, never panic — a panic kills the runner task until restart.

### Wiring (consuming apps)

```rust
// 1. Register executors (in Hooks::routes(), before initializers run):
fracture_core::jobs::init_job_registry(my_registry());

// 2. Add the runner (in Hooks::initializers()):
Box::new(fracture_core::jobs::runner::JobRunnerInitializer)
```

Settings (`config/*.yaml`):

```yaml
settings:
  jobs:
    enabled: true              # default true; set false in test configs
    poll_interval_seconds: 15  # default 15
```

### Authorization (configurable, staff-managed)

Job actions fall into three buckets — **view** (list/detail/run history), **run** (trigger a run), and **manage** (create / edit / delete / enable-disable) — and each has a configurable minimum [`JobAccessLevel`] (`fracture_core::jobs::permissions`): Viewer / Member / Admin / Owner / **Staff** (platform-staff only). Platform staff clear every level (the unconditional ceiling). The policy is global and lives in the platform-admin org's settings (no new table); staff edit it at **`/admin/job-permissions`**, and the handlers + templates both gate on the resolved [`JobAccess`].

**Default policy is tenant view-only:** `view = Viewer` (any member), `run = Staff`, `manage = Staff` — so out of the box, tenants can watch jobs but only platform staff run or manage them. Loosen per deployment as needed. Create and edit still share one validator (registered job type, cron-with-seconds, JSON config, unique name per org); the job type is fixed once created. Platform admins can additionally open any org's definitions read-only from `/admin/jobs`.

### Example executors (reference app)

The demo crate registers two executors in `src/jobs.rs` as templates for consumers:

- **`content_stats`** — read-only: counts the org's projects and notes and diffs against the previous run. Shows the summary + diff contract without side effects.
- **`write_note`** — side-effecting: each run writes a note into the org (creating a holding project if none exists) and reports a `created` diff. The smallest end-to-end proof of the lifecycle — run it manually from a job's page and watch the queued run execute, persist a row, and surface the diff on the run page. Optional config `{ "title": "…" }` sets the note title prefix.

Consuming apps add their own executors the same way (`registry.register(Box::new(MyJob))`); fracture-core imposes no limit on what an executor does.

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
| GET | `/jobs` | Authenticated | List job definitions + last-run status for current org |
| POST | `/jobs` | Org admin | Create a definition (validates job type, cron, JSON config) |
| GET | `/jobs/{pid}` | Authenticated | Show definition + runs |
| POST | `/jobs/{pid}/toggle` | Org admin | Enable/disable a definition |
| POST | `/jobs/{pid}/run` | Org member | Trigger a queued run (no-op while one is active) |
| GET | `/jobs/{pid}/runs/{run_pid}` | Authenticated | Show a run + its diffs |
| GET | `/admin/jobs` | Platform admin | List all definitions across all orgs |

## Contact Form & Captcha

`/contact` (public layout, session-aware CTA) stores submissions in the
`contact_messages` table; platform admins triage at `/admin/contact`. Spam
control is a self-hosted [Altcha](https://altcha.org) proof-of-work captcha —
no third-party calls:

- `captcha.rs` issues SHA-256 PoW challenges signed with HMAC-SHA256 under a
  boot-ephemeral key (`GET /captcha/challenge`), and verifies solutions
  (solution hash, signature, expiry embedded in the salt, single-use replay
  store). HMAC is implemented per RFC 2104 over the existing `sha2` dep and
  pinned by RFC 4231 test vectors.
- The widget is vendored at `assets/static/altcha.{min.js,worker.js,css}`
  (the `dist_external` CSP-friendly build — external worker file, external
  CSS, so `script-src 'self'`/`style-src 'self'` hold). Downstream apps must
  ship these three static files (same rule as `oat.min.*`).
- Any other public form can reuse the gate: embed the widget pointing at
  `/captcha/challenge` and call `fracture_core::captcha::verify_payload` on
  the submitted `altcha` field.

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
6. New users: `find_or_create_from_oidc()` calls `find_pending_by_email()` and auto-accepts matching invites on first login — only when the login's email is verified (IdP `email_verified` claim, or the `assume_email_verified` config opt-in for IdPs that never emit it). Unverified signups keep their invites pending.

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
