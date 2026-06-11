#!/bin/bash
set -euo pipefail

COMPOSE="podman compose"
ZITADEL_API="http://localhost:8080"

# Authenticated API helper — passes PAT automatically.
zapi() {
    local method="$1" path="$2"
    shift 2
    curl -s -X "$method" "$ZITADEL_API$path" \
        -H "Authorization: Bearer $PAT" \
        -H "Content-Type: application/json" \
        "$@"
}

# --- 1. Start Zitadel + Postgres ---
echo "==> Starting Zitadel and database..."
$COMPOSE up -d zitadel-db zitadel

echo "    Waiting for readiness..."
until curl -s "$ZITADEL_API/debug/ready" > /dev/null 2>&1; do sleep 2; done
echo "    Zitadel is ready."

# --- 2. Get PAT from logs ---
echo "==> Retrieving admin PAT..."
# Zitadel prints the machine user PAT as a raw token on its own line
PAT=""
for _ in $(seq 1 10); do
    PAT=$($COMPOSE logs zitadel 2>&1 \
        | grep -E '^[A-Za-z0-9_-]{30,}$' \
        | head -1) || true
    if [ -n "$PAT" ]; then break; fi
    sleep 2
done

if [ -z "$PAT" ]; then
    echo "ERROR: Could not find PAT in Zitadel logs."
    echo "       Check: podman compose logs zitadel"
    exit 1
fi
echo "    PAT retrieved."

# --- 3. Use v1 login UI (v2 is not bundled in self-hosted image) ---
echo "==> Configuring login UI..."
zapi PUT /v2/features/instance \
    -d '{"loginV2":{"required":false}}' > /dev/null

# --- 3b. Configure SMTP (env vars only apply on first-ever init) ---
echo "==> Configuring SMTP for MailCrab..."
SMTP_ID=$(curl -s -X POST "$ZITADEL_API/admin/v1/smtp" \
    -H "Authorization: Bearer $PAT" \
    -H "Content-Type: application/json" \
    -d '{
        "senderAddress": "noreply@fracture-cms.local",
        "senderName": "Fracture CMS",
        "host": "mailcrab:1025",
        "user": "",
        "password": "",
        "tls": false
    }' | jq -r '.id')
curl -s -X POST "$ZITADEL_API/admin/v1/smtp/$SMTP_ID/_activate" \
    -H "Authorization: Bearer $PAT" \
    -H "Content-Type: application/json" > /dev/null
echo "    SMTP configured (mailcrab:1025)."

# --- 4. Create project ---
echo "==> Creating project 'Fracture CMS'..."
PROJECT_ID=$(zapi POST /management/v1/projects \
    -d '{"name":"Fracture CMS"}' | jq -r '.id')
echo "    Project ID: $PROJECT_ID"

# --- 5. Create OIDC application ---
echo "==> Creating OIDC application..."
APP_RESPONSE=$(zapi POST "/management/v1/projects/$PROJECT_ID/apps/oidc" \
    -d '{
        "name": "Fracture CMS",
        "redirectUris": ["http://localhost:5150/api/auth/oidc/callback"],
        "responseTypes": ["OIDC_RESPONSE_TYPE_CODE"],
        "grantTypes": ["OIDC_GRANT_TYPE_AUTHORIZATION_CODE"],
        "appType": "OIDC_APP_TYPE_WEB",
        "authMethodType": "OIDC_AUTH_METHOD_TYPE_BASIC",
        "postLogoutRedirectUris": ["http://localhost:5150"],
        "devMode": true,
        "idTokenUserinfoAssertion": true,
        "backChannelLogoutUri": "http://host.containers.internal:5150/api/auth/oidc/backchannel-logout"
    }')

CLIENT_ID=$(echo "$APP_RESPONSE" | jq -r '.clientId')
CLIENT_SECRET=$(echo "$APP_RESPONSE" | jq -r '.clientSecret')
echo "    Client ID: $CLIENT_ID"

# --- 6. Create test user ---
echo "==> Creating test user..."
TEST_PASS="TestPassword1!"
zapi POST /management/v1/users/human \
    -d "{
        \"userName\": \"testuser\",
        \"profile\": {
            \"firstName\": \"Test\",
            \"lastName\": \"User\",
            \"displayName\": \"Test User\"
        },
        \"email\": {
            \"email\": \"testuser@example.com\",
            \"isEmailVerified\": true
        },
        \"initialPassword\": \"$TEST_PASS\"
    }" > /dev/null

# --- 7. Write .env ---
echo "==> Writing .env..."
JWT_SECRET=$(openssl rand -base64 32)
# Create the file with restrictive permissions before writing secrets into it,
# so JWT_SECRET / OIDC_CLIENT_SECRET are never world-readable (matches the 0600
# that `fracture-ctl init` sets for the prod .env).
install -m 600 /dev/null .env
cat > .env <<EOF
JWT_SECRET=$JWT_SECRET
OIDC_PROJECT_ID=$PROJECT_ID
OIDC_CLIENT_ID=$CLIENT_ID
OIDC_CLIENT_SECRET=$CLIENT_SECRET
EOF

# --- Done ---
echo ""
echo "========================================"
echo "  Dev Environment Setup Complete"
echo "========================================"
echo ""
echo "  Zitadel (IdP):   http://localhost:8080"
echo "  MailCrab (Email): http://localhost:1080"
echo "  App:             http://localhost:5150"
echo ""
echo "  Test user:       testuser"
echo "  Test password:   $TEST_PASS"
echo ""
echo "  Next steps:"
echo "    podman compose up -d mailcrab app"
echo "    curl http://localhost:5150/api/auth/oidc/providers"
echo "========================================"
