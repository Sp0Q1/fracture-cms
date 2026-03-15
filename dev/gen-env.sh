#!/bin/bash
# Generate a production .env.prod file with secure random secrets.
# Usage: ./dev/gen-env.sh > .env.prod && chmod 600 .env.prod
set -euo pipefail

cat <<EOF
# Fracture CMS — Production environment
# Generated $(date -u +%Y-%m-%dT%H:%M:%SZ)

JWT_SECRET=$(openssl rand -base64 32)

# Database — SQLite by default. Uncomment for PostgreSQL:
# APP_DB_PASSWORD=$(openssl rand -base64 24)
# DATABASE_URL=postgres://fracture:\${APP_DB_PASSWORD}@db:5432/fracture

# OIDC — optional. App works without it (login returns 503).
# Configure your identity provider and fill these in:
# OIDC_ISSUER_URL=https://auth.example.com
# OIDC_CLIENT_ID=
# OIDC_CLIENT_SECRET=
# OIDC_REDIRECT_URI=https://example.com/api/auth/oidc/callback
# OIDC_POST_LOGOUT_REDIRECT_URI=https://example.com

# SMTP — optional. Invite emails fail silently if not configured.
# MAILER_HOST=smtp.example.com
# MAILER_PORT=587
# MAILER_USER=
# MAILER_PASSWORD=
EOF
