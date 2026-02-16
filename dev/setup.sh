#!/bin/bash
set -euo pipefail

COMPOSE="podman compose"
KANIDM_API="https://localhost:8443"

# Authenticate against the kanidm REST API, return a bearer token.
# The 3-step auth flow uses the x-kanidm-auth-session-id header (a JWT)
# to track the session across requests.
kanidm_auth() {
    local user="$1"
    local pass="$2"
    local hdr
    hdr=$(mktemp)

    # Step 1 – init: declare which account we're authenticating as
    curl -sk -D "$hdr" -o /dev/null -X POST "$KANIDM_API/v1/auth" \
        -H "Content-Type: application/json" \
        -d "{\"step\":{\"init\":\"$user\"}}"
    local sid
    sid=$(grep -i x-kanidm-auth-session-id "$hdr" | tr -d '\r' | awk '{print $2}')

    # Step 2 – begin: select "password" mechanism
    curl -sk -D "$hdr" -o /dev/null -X POST "$KANIDM_API/v1/auth" \
        -H "Content-Type: application/json" \
        -H "x-kanidm-auth-session-id: $sid" \
        -d '{"step":{"begin":"password"}}'
    sid=$(grep -i x-kanidm-auth-session-id "$hdr" | tr -d '\r' | awk '{print $2}')

    # Step 3 – cred: submit password via heredoc (stays out of process args)
    local resp
    resp=$(curl -sk -X POST "$KANIDM_API/v1/auth" \
        -H "Content-Type: application/json" \
        -H "x-kanidm-auth-session-id: $sid" \
        --data-binary @- <<EOF
{"step":{"cred":{"password":"$pass"}}}
EOF
    )
    rm -f "$hdr"

    echo "$resp" | jq -r '.state.success'
}

# Authenticated API helper – passes bearer token automatically.
api() {
    local token="$1" method="$2" path="$3"
    shift 3
    curl -sk -o /dev/null -X "$method" "$KANIDM_API$path" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer $token" \
        "$@"
}

# --- 1. Generate TLS certificates ---
echo "==> Generating TLS certificates..."
$COMPOSE run --rm kanidm-init 2>&1

# --- 2. Start kanidm ---
echo "==> Starting kanidm..."
$COMPOSE up -d kanidm
echo "    Waiting for readiness..."
sleep 3
until curl -sk "$KANIDM_API/status" > /dev/null 2>&1; do sleep 2; done
echo "    Kanidm is ready."

# --- 3. Recover built-in account passwords ---
echo "==> Recovering admin password..."
ADMIN_PASS=$($COMPOSE exec -T kanidm kanidmd recover-account admin 2>&1 \
    | grep -oP 'password: \K\S+' | tr -d '"')

echo "==> Recovering idm_admin password..."
IDM_ADMIN_PASS=$($COMPOSE exec -T kanidm kanidmd recover-account idm_admin 2>&1 \
    | grep -oP 'password: \K\S+' | tr -d '"')

# --- 4. Authenticate via REST API ---
echo "==> Authenticating as admin..."
ADMIN_TOKEN=$(kanidm_auth admin "$ADMIN_PASS")

echo "==> Authenticating as idm_admin..."
IDM_TOKEN=$(kanidm_auth idm_admin "$IDM_ADMIN_PASS")

# --- 5. Create OAuth2 client ---
echo "==> Creating OAuth2 client 'fracture-cms'..."
api "$IDM_TOKEN" POST /v1/oauth2/_basic \
    -d '{"attrs":{
        "name":["fracture-cms"],
        "displayname":["Fracture CMS"],
        "oauth2_rs_origin_landing":["http://localhost:5150/api/auth/oidc/callback"]
    }}'

echo "==> Configuring scope map..."
api "$IDM_TOKEN" POST /v1/oauth2/fracture-cms/_scopemap/idm_all_persons \
    -d '["openid","email","profile"]'

# Localhost redirects are implicit since our origin is already localhost.

echo "==> Preferring short usernames..."
api "$IDM_TOKEN" PATCH /v1/oauth2/fracture-cms \
    -d '{"attrs":{"oauth2_prefer_short_username":["true"]}}'

echo "==> Retrieving client secret..."
CLIENT_SECRET=$(curl -sk "$KANIDM_API/v1/oauth2/fracture-cms/_basic_secret" \
    -H "Authorization: Bearer $IDM_TOKEN" | jq -r '.')

# --- 6. Create test user and group ---
echo "==> Creating group 'fracture_users'..."
api "$IDM_TOKEN" POST /v1/group \
    -d '{"attrs":{"name":["fracture_users"]}}'

echo "==> Creating person 'testuser'..."
api "$IDM_TOKEN" POST /v1/person \
    -d '{"attrs":{"name":["testuser"],"displayname":["Test User"]}}'

echo "==> Setting test user email..."
api "$IDM_TOKEN" PATCH /v1/person/testuser \
    -d '{"attrs":{"mail":["testuser@example.com"]}}'

echo "==> Adding testuser to fracture_users..."
api "$IDM_TOKEN" POST /v1/group/fracture_users/_attr/member \
    -d '["testuser"]'

echo "==> Recovering testuser password..."
TEST_PASS=$($COMPOSE exec -T kanidm kanidmd recover-account testuser 2>&1 \
    | grep -oP 'password: \K\S+' | tr -d '"')

# --- 7. Write .env ---
echo "==> Writing .env..."
JWT_SECRET=$(openssl rand -base64 32)
cat > .env <<EOF
JWT_SECRET=$JWT_SECRET
OIDC_CLIENT_SECRET=$CLIENT_SECRET
EOF

# --- Done ---
echo ""
echo "========================================"
echo "  Dev Environment Setup Complete"
echo "========================================"
echo ""
echo "  Kanidm (IdP):    https://localhost:8443"
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
