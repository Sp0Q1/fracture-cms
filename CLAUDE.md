# CLAUDE.md — fracture-cms

These rules are non-negotiable for any code change in this repo. They exist so that semgrep, the strict CSP, and the framework conventions never regress. Read this file before editing.

## What this project is

Rust + Loco (Axum) CMS framework. Three crates:

- `fracture-core/` — library re-exported to downstream crates (notably `fracture-pt`). Owns auth, RBAC, OIDC, multi-tenancy, jobs, mailers, security headers.
- `fracture-ctl/` — IdP-agnostic CLI for production deployment (config gen, container orchestration).
- root crate — the demo/reference app (projects, notes) that exercises `fracture-core`.

`fracture-pt` consumes `fracture-core` as a git dependency. **CMS must not depend on PT.** The dependency direction is one-way: PT → CMS.

## Build & test commands

Use **local cargo** (Rust 1.94+ installed). Podman is only for semgrep and the running app stack.

| Task | Command |
|---|---|
| Format check | `cargo fmt --all -- --check` |
| Format apply | `cargo fmt --all` |
| Lint (matches CI) | `cargo clippy --all-features -- -D warnings -W clippy::pedantic -W clippy::nursery -W rust-2018-idioms` |
| Tests | `DATABASE_URL=sqlite:///tmp/fracture-cms_test.sqlite?mode=rwc cargo test --all-features --all` |
| Full local CI | `./dev/ci.sh` |
| Semgrep only | `podman run --rm -v "$PWD:/src:ro" -w /src docker.io/semgrep/semgrep:latest semgrep scan --config auto --error --exclude-rule python.django.security.django-no-csrf-token.django-no-csrf-token .` |
| Audit | `cargo audit` |
| Run app stack | `podman compose up -d` |

`./dev/ci.sh` MUST pass before opening a PR.

## Pipelines that must always succeed

CI in `.github/workflows/ci.yaml` runs:

1. `rustfmt` — formatting check
2. `clippy` — strict lints (pedantic + nursery + rust-2018-idioms, warnings denied)
3. `semgrep` — auto config, errors fail the job
4. `cargo test --all-features --all`
5. Audit (in `release-app.yaml` and `audit.yaml`)

If a clippy or semgrep finding cannot be fixed, justify it inline:

- Clippy: `#[allow(clippy::lint_name)] // Reason: <explanation>`
- Semgrep: `// nosemgrep: <rule-id> -- <reason>` with a one-line justification on the line above the offending code.

Never use `--no-verify` to skip hooks. Never bypass signing.

## Security rules (mandatory)

### Authorization (highest priority — IDOR prevention)

- **Every resource that belongs to an organization must implement org-scoped lookups.** The pattern: `Model::find_by_pid_and_org(db, pid, org_id)`. After PR-3 lands, this becomes the `OrgScoped` trait — implement it for every new entity.
- **`find_by_pid` (without org check) is internal-only.** Treat it as `find_by_pid_unchecked`. If you call it from a controller, you must independently verify authorization in that handler. Prefer the org-scoped helper in 99% of cases.
- **Use the auth macros, never inline checks:**
  - `require_user!(user)` to ensure authentication
  - `require_role!(org_ctx, OrgRole::Admin)` to enforce role
  - `require_platform_admin!(org_ctx)` for platform-admin gates
- **Return 404 (not 403) on unauthorized access** so endpoint existence is not leaked. Loco's `not_found()` is the standard exit.
- **Roles live in `fracture-core/src/models/org_members.rs`.** Adding a generic per-resource role goes through the `ResourceAssignment` model (PR-4). Do not add new variants to `OrgRole` for engagement-scoped or per-resource purposes — those go through `ResourceAssignment`.

### Content Security Policy

The CSP in `fracture-core/src/initializers/security_headers.rs` must remain strict:

```
default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self';
font-src 'self'; connect-src 'self'; form-action 'self'; base-uri 'self';
frame-ancestors 'none'
```

**Forbidden:**
- `unsafe-inline` in `script-src` or `style-src`
- `unsafe-eval`
- Wildcard origins (`*`) anywhere
- Removing `frame-ancestors 'none'`

If a feature genuinely needs an inline script, use a per-request CSP nonce; never relax the directive globally.

### SRI (subresource integrity)

All `<link rel="stylesheet">` and `<script>` tags loaded from `/static/` must include `integrity="sha384-..."` and `crossorigin="anonymous"`. Update the hash when the file changes. PT already enforces this; CMS templates must reach parity.

### Headers

Do not weaken any of: `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy: strict-origin-when-cross-origin`, `Strict-Transport-Security: max-age=63072000; includeSubDomains`, `X-Permitted-Cross-Domain-Policies: none`, `Permissions-Policy: camera=() microphone=() geolocation=()`.

### Database

- **No raw SQL.** SeaORM only. If a use case truly requires raw SQL, it goes through `Statement::from_sql_and_values` with bound parameters and a code-review note.
- **No string interpolation into queries.** All filters use SeaORM column expressions.
- **Migrations** must include `org_id` (non-null, FK, on delete cascade) for any new org-owned table, plus an index on `org_id`.

### Templates

- Tera autoescape is **on**. Never bypass with `| safe` for user-provided content. The only safe consumers of `| safe` are server-rendered HTML known not to contain user input.
- Markdown rendering uses `comrak` with `options.render.unsafe = false`. Do not flip this.
- Inline `<script>` and inline `<style>` are forbidden (they violate CSP). Use `/static/` files.

### Secrets

- All secrets via env vars; never hardcoded. `.env` is gitignored; `.env.example` lists required vars.
- `JWT_SECRET` must be base64-decodable (the OIDC initializer assumes this).
- **Never use `$(cat secret-file)` or `$(...)` to substitute secrets into commands** — see global rule. Use the gh CLI's built-in auth, env vars in compose, or k8s secrets.

### OIDC

- PKCE, nonce, JWKS verification, audience-claim check stay enabled. Do not remove.
- If `auth.oidc.client_id` / `issuer` / `client_secret` is missing in production, fail loudly — do not silently disable. (Pending PR-12.)

### Sessions / cookies

- HttpOnly, SameSite=Lax, Secure (in HTTPS), 15-min default TTL, server-side `session_invalidated_at` for revocation. Do not relax.

## Framework conventions

- **Loco 0.16, Axum 0.8, SeaORM 1.1, Tera, Fluent for i18n.** When adding a feature, follow the existing module shapes:
  - `controllers/<resource>.rs` — handlers + routes
  - `models/<resource>.rs` — domain methods, keeping `_entities/<resource>.rs` (auto-generated) untouched
  - `views/<resource>.rs` — Tera context builders
  - `assets/views/<resource>/...html` — templates
  - Migration in `fracture-core/migration/src/m<date>_<name>.rs`, registered in `lib.rs`
- **Use `chrono::Utc::now()`** consistently. Don't mix `Local`/`FixedOffset`.
- **Logging via `tracing`**, structured fields preferred over interpolated strings.
- **Errors** propagate via `?`. Avoid `unwrap`/`expect` outside startup-only init paths. Never `panic!()` in request paths.

## Testing

Every PR must:

- Add or update tests for new logic. Integration tests live in `tests/requests/`, model tests in `tests/models/`.
- Pass `cargo test --all-features --all` locally.
- For new org-scoped resources: include a test proving a user from org A cannot read a resource owned by org B (the IDOR prevention test).

## Manual local testing (mandatory before marking work complete)

Automated tests verify code correctness; they do not verify feature correctness in a browser. After every change that touches a request handler, view, template, asset, or the data model:

1. Bring the stack up against the working tree: `podman compose down app && podman compose build app && podman compose up -d app` (or `podman compose up -d` for a fresh start).
2. Watch logs while exercising the change: `podman compose logs -f app`.
3. Open the affected pages in a browser and walk both the golden path and at least one edge case.
4. Verify there is no CSP violation in the browser console (every regression there is a security failure).
5. If the change touches auth or RBAC, log in as users with each affected role and confirm the gate behaves correctly — including the negative case (a role that should NOT see the resource).
6. If the change touches a migration, confirm `auto_migrate` runs cleanly on a fresh DB and on a DB seeded from the previous schema.

Do not mark a task or PR complete on the basis of `cargo test` alone. If the manual test cannot be run (e.g., requires external IdP credentials you do not have), say so explicitly in the PR description rather than claiming success.

## Documentation

- README — high-level only.
- `docs/ARCHITECTURE.md` — must be updated for changes that alter the auth model, the `OrgScoped` contract, or extension points consumed by downstream crates.
- `docs/ADDING_RESOURCES.md` — must be updated when the consumer recipe changes.
- `docs/UPSTREAM_UPDATES.md` — keep current for downstream rebasers.

## PR hygiene

- One concern per PR. Don't bundle unrelated cleanups.
- PR title: imperative, < 70 chars.
- PR body: summary + test plan checklist.
- PRs must:
  1. Pass CI.
  2. Include tests for new logic.
  3. Update docs if behavior or contract changes.
  4. Not depend on `main` branch of any git dependency without explicit reason in the description.
- Never force-push or amend after merge. New PR for follow-ups.

## Don't do

- Don't add features beyond the task's scope (no surrounding cleanups in a bug-fix PR).
- Don't introduce new dependencies without a stated reason in the PR description.
- Don't widen the CSP for convenience.
- Don't add inline scripts or styles to templates.
- Don't bypass `OrgScoped` lookups in controllers.
- Don't `unwrap()` in request handlers.
- Don't add panics or `todo!()` in production code paths.
