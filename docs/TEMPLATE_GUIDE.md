# Template / Fork Guide

Fracture CMS is designed as a reusable template for downstream projects. This guide explains how to fork it and keep your fork up-to-date with upstream changes.

## Forking

```bash
# Clone the template
git clone <fracture-cms-repo> my-project
cd my-project

# Rename origin to upstream
git remote rename origin upstream

# Add your own remote
git remote add origin <my-project-repo>
git push -u origin main
```

## Pulling Upstream Updates

```bash
git fetch upstream
git merge upstream/main
# Resolve any conflicts (see below for which files to expect conflicts in)
```

## What to Keep vs. Customize

### Template Core (accept upstream updates)

These files implement the org/auth/RBAC infrastructure. Take upstream updates for these:

- `src/controllers/middleware.rs` — OrgContext, RBAC macros
- `src/controllers/oidc.rs` — OIDC authentication flow
- `src/controllers/org.rs` — Organization management
- `src/models/organizations.rs` — Org model logic
- `src/models/org_members.rs` — Membership + OrgRole
- `src/models/org_invites.rs` — Invite flow
- `src/models/_entities/organizations.rs`, `org_members.rs`, `org_invites.rs`
- `src/mailers/invite.rs` — Invitation email mailer
- `src/initializers/` — OIDC, view engine, security headers
- `migration/src/m20260220_*` — Org/RBAC migrations

### Customize Zone (expect merge conflicts)

These files are where you add your domain-specific logic:

- `src/controllers/project.rs`, `note.rs` — Replace with your resources
- `src/models/projects.rs`, `notes.rs` — Replace with your models
- `src/views/project.rs`, `note.rs` — Replace with your views
- `assets/views/project/`, `note/` — Replace with your templates
- `assets/views/base.html` — Nav links, branding
- `assets/views/home/index.html` — Dashboard content
- `assets/static/app.css` — Colors, custom component styles
- `assets/static/app.js` — Add new `data-` attribute handlers for your features
- `src/app.rs` — Route registration, truncate order
- `src/mailers/` — Add your own mailers alongside invite.rs
- `README.md` — Project-specific documentation

## Adding a New Org-Scoped Resource

See [ADDING_RESOURCES.md](ADDING_RESOURCES.md) for a step-by-step checklist.
