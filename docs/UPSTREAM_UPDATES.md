# Pulling Upstream Updates

## Setup (one-time)

```bash
git remote add upstream <fracture-cms-repo-url>
```

## Pulling Updates

```bash
git fetch upstream
git merge upstream/main
```

## Expected Merge Conflicts

When merging upstream changes, you'll likely see conflicts in these files:

### Always conflicting (your domain-specific files)
- `src/app.rs` — Route registration, truncate order
- `src/controllers/mod.rs` — Module declarations
- `src/models/mod.rs` — Module declarations
- `src/views/mod.rs` — Module declarations
- `assets/views/base.html` — Nav links
- `README.md` — Project description

### Sometimes conflicting (shared + customized)
- `assets/static/app.css` — If you changed colors or added styles near the same lines
- `assets/static/app.js` — If you added data-attribute handlers near the same lines
- `assets/views/home/index.html` — If upstream changed the dashboard layout
- `Cargo.toml` — If both sides added dependencies

### Rarely conflicting (template core)
- `src/controllers/middleware.rs`
- `src/controllers/oidc.rs`
- `src/models/organizations.rs`
- `src/models/org_members.rs`
- `src/models/org_invites.rs`
- `src/mailers/invite.rs` — Invite email mailer
- `migration/src/lib.rs` — Only if both sides added migrations

## Resolution Strategy

1. For `mod.rs` files: combine both sets of module declarations
2. For `app.rs`: keep your routes + upstream's routes, maintain truncate order (children before parents)
3. For `base.html`: keep your nav links, take upstream's structural changes
4. For `app.css`: keep your custom styles, take upstream's new component styles
5. For model/controller core: prefer upstream unless you intentionally modified the behavior
