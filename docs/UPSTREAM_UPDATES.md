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
