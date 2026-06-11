# Updating fracture-core

Since `fracture-core` is a Cargo dependency, updating to the latest version is straightforward — no merge conflicts on core infrastructure code.

## Updating

```bash
cargo update -p fracture-core
cargo update -p fracture-core-migration
```

This pulls the latest version from the git repository. Then rebuild:

```bash
cargo build
```

If there are breaking API changes, the compiler will tell you exactly what needs updating.

## What Can Change

### Non-breaking (transparent)

These changes are pulled in automatically with no code changes needed:

- Bug fixes in OIDC flow, org management, RBAC logic
- New security headers or CSP improvements
- Updated embedded templates (unless you've overridden them)
- Internal refactors that don't change the public API

### Potentially breaking

These may require code changes in your app:

- New required parameters on controller functions
- Changed model method signatures (e.g., `find_or_create_from_oidc`)
- New fields on entities (requires migration coordination)
- Renamed or removed public types/functions

## Recent Breaking Changes

### The jobs system now has a runner — two wiring hooks are load-bearing

Previously `JobExecutor`/`JobRegistry` existed but nothing executed queued
runs. Now `fracture_core::jobs::runner::JobRunnerInitializer` polls for
queued `job_runs`, executes them via the registry, evaluates cron
`schedule`s, and persists outcomes. To adopt it:

1. Call `fracture_core::jobs::init_job_registry(...)` in your `Hooks::routes()`.
2. Add `Box::new(fracture_core::jobs::runner::JobRunnerInitializer)` to
   `Hooks::initializers()`.
3. Optionally configure `settings.jobs.{enabled, poll_interval_seconds}`;
   set `enabled: false` in test configs.

Without step 1, runs fail with "no executor registered"; without step 2,
runs stay queued forever (the old behavior). Route changes: `POST /jobs`
(create, Admin) and `POST /jobs/{pid}/toggle` (Admin) are new;
`POST /jobs/{pid}/run` now requires Member+ (was: any authenticated user),
refuses disabled definitions, and is a no-op while a run is already active.

### `OidcUserInfo` gained a required `email_verified: bool` field

If you construct `OidcUserInfo` yourself, you must now supply
`email_verified`. **Pass the IdP's actual `email_verified` claim** (use
`claims.email_verified().unwrap_or(assume_email_verified)` where
`assume_email_verified` is an operator config opt-in, default `false`).
Do **not** hardcode `true`: this flag gates linking OIDC logins to existing
email-matched accounts and auto-accepting pending invites — hardcoding it
reopens the account-takeover vector the field exists to close (an attacker
registering an unverified victim@example.com at the IdP).

### `org_members::Model::update_role` / `remove_member` signatures changed

Both now take `(db, org_id, target_user_id, actor_role, ...)` instead of a
pre-fetched membership row, return `MemberWriteError` instead of `DbErr`, and
enforce the role ceiling (actor must outrank-or-equal the target's current
role and the granted role) inside the write transaction. Map
`MemberWriteError::NotFound`/`Forbidden` to your 404 path and surface
`LastOwner` as a user-facing message — see `controllers/org.rs` for the
canonical mapping.

### Migration changes

If `fracture-core` adds new migrations, they are automatically picked up — your `migration/src/lib.rs` chains `fracture_core_migration::Migrator::migrations()` first, then appends your app-specific migrations.

New core migrations will run automatically on the next app startup (if `auto_migrate: true`) or when you run migrations manually.

## Pinning a Version

To pin to a specific commit instead of always pulling latest:

```toml
[dependencies]
fracture-core = { git = "https://your-repo/fracture-core.git", rev = "abc1234" }
```

Or use a tag:

```toml
[dependencies]
fracture-core = { git = "https://your-repo/fracture-core.git", tag = "v0.2.0" }
```

## Checking What Changed

```bash
# See what version you currently have
cargo metadata --format-version 1 | jq '.packages[] | select(.name == "fracture-core") | .source'

# After updating, check for compile errors
cargo check
```
