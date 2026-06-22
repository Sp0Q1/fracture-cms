#!/bin/bash
set -euo pipefail

# Dev environment setup for the Keycloak-based stack.
#
# Realms are imported declaratively by Keycloak on boot (see
# dev/keycloak/import), so there is no admin-API provisioning here — this
# script only starts the services, waits for readiness, and writes .env with
# the (static, dev-only) client credentials baked into the import files.

COMPOSE="podman compose"
TENANT_REALM="tenant-acme"
ISSUER="http://localhost:8080/realms/${TENANT_REALM}"

# These are the dev-only static values defined in the realm import JSON.
CLIENT_ID="fracture-cms"
CLIENT_SECRET="fracture-dev-secret"

echo "==> Starting Keycloak and MailCrab..."
$COMPOSE up -d keycloak mailcrab

echo "    Waiting for Keycloak to import realms and serve discovery..."
until curl -sf "${ISSUER}/.well-known/openid-configuration" > /dev/null 2>&1; do
    sleep 2
done
echo "    Keycloak is ready (realm '${TENANT_REALM}' discoverable)."

echo "==> Writing .env..."
JWT_SECRET=$(openssl rand -base64 32)
# Restrictive perms before writing secrets (matches fracture-ctl init's 0600).
install -m 600 /dev/null .env
cat > .env <<EOF
JWT_SECRET=${JWT_SECRET}
OIDC_PROJECT_ID=
OIDC_CLIENT_ID=${CLIENT_ID}
OIDC_CLIENT_SECRET=${CLIENT_SECRET}
EOF

echo ""
echo "========================================"
echo "  Dev Environment Setup Complete"
echo "========================================"
echo ""
echo "  Keycloak (IdP):   http://localhost:8080  (admin / admin)"
echo "  MailCrab (email): http://localhost:1080"
echo "  App:              http://localhost:5150"
echo ""
echo "  Tenant realm:     ${TENANT_REALM}"
echo "    Tenant user:    owner@acme.example / password"
echo "  Staff realm:      staff (brokered into the tenant realm)"
echo "    Staff user:     staff@example.com / password"
echo "                    → on the app login page, choose \"Staff sign-in\""
echo ""
echo "  Start the app:    podman compose up -d app   (or run it locally)"
echo ""
echo "  Note: Keycloak supports back-channel logout; the app's"
echo "  /api/auth/oidc/backchannel-logout endpoint is registered on the"
echo "  fracture-cms client. To exercise it, bind the app on all interfaces:"
echo "      SERVER_BINDING=0.0.0.0 podman compose up -d app"
