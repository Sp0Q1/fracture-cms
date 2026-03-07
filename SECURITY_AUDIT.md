# Security Audit Report — Fracture CMS

**Date:** 2026-03-03
**Auditor:** Security Tester (product-team)
**Scope:** Full codebase review of fracture-cms (fracture-core + root crate)
**Methodology:** Manual static analysis of all controllers, models, views, templates, initializers, and client-side JavaScript.

---

## Executive Summary

Fracture CMS demonstrates strong security fundamentals: OIDC with PKCE, org-scoped data isolation via SeaORM parameterized queries, role-based access control on all endpoints, strict CSP headers, and HTTP-only cookies. However, several medium-severity issues were identified, primarily around missing cookie `Secure` flags, absence of CSRF protection on state-changing forms, no rate limiting, and a potential open-redirect in the logout flow.

**Fixes Applied:** HIGH-2, MEDIUM-1, MEDIUM-4, MEDIUM-5 have been fixed directly in the code. HIGH-1 (CSRF tokens) and MEDIUM-2/MEDIUM-3 (rate limiting) are noted for future implementation.

**Finding Summary:**
| Severity | Count | Fixed |
|----------|-------|-------|
| Critical | 0     | -     |
| High     | 2     | 1     |
| Medium   | 6     | 4     |
| Low      | 4     | 2     |
| Info     | 4     |

---

## Findings

### HIGH-1: No CSRF Protection on State-Changing Form Submissions

**Severity:** High
**Category:** OWASP A01:2021 — Broken Access Control
**Location:** All POST form endpoints across the application

**Description:**
None of the HTML forms include CSRF tokens. The application relies on `SameSite=Lax` cookies as the sole CSRF defense. While `SameSite=Lax` blocks cross-site POST submissions from third-party sites in modern browsers, this is insufficient because:

1. `SameSite=Lax` only protects against cross-site POSTs originating from top-level navigations; it does not protect against all attack vectors (e.g., subdomain attacks if the app shares a registrable domain).
2. Older browsers may not enforce `SameSite` correctly.
3. Defense-in-depth best practice requires a synchronizer token or double-submit cookie pattern.

**Affected endpoints:**
- `POST /orgs/` (create org)
- `POST /orgs/:pid/settings` (update org)
- `POST /orgs/:pid/members/invite` (invite member)
- `POST /orgs/:pid/members/:user_pid/role` (change role)
- `POST /orgs/:pid/members/:user_pid/remove` (remove member)
- `POST /projects/` (create project)
- `POST /projects/:pid` (update project)
- `DELETE /projects/:pid` (delete project)
- `POST /projects/:project_pid/notes/` (create note)
- `POST /projects/:project_pid/notes/:pid` (update note)
- `DELETE /projects/:project_pid/notes/:pid` (delete note)

**Recommendation:**
Implement CSRF tokens on all state-changing endpoints. Options:
- Add a synchronizer token pattern (generate per-session token, embed in forms, validate on POST).
- Use the double-submit cookie pattern (set a random value in a non-HttpOnly cookie, require it as a form field).

---

### HIGH-2: Missing `Secure` Flag on All Cookies -- FIXED

**Severity:** High (FIXED)
**Category:** OWASP A02:2021 — Cryptographic Failures
**Location:** `fracture-core/src/controllers/oidc.rs` (lines 141-162, 205-216, 269-273), `fracture-core/src/controllers/org.rs` (line 378-382)

**Description:**
All cookies (`jwt`, `id_token`, `org_pid`) are set without the `Secure` flag. In production over HTTPS, the browser will still send these cookies over plain HTTP connections (e.g., if a user visits an HTTP URL or an attacker performs an SSL-stripping attack). The JWT session token sent in cleartext allows session hijacking.

**Affected cookies:**
- `jwt` — session token (all set-cookie locations)
- `id_token` — OIDC ID token for logout hint
- `org_pid` — active organization selector

**Code example (`oidc.rs:141-145`):**
```rust
let jwt_cookie = Cookie::build(("jwt", token))
    .path("/")
    .http_only(true)
    .same_site(SameSite::Lax)
    // Missing: .secure(true)
    .build();
```

**Recommendation:**
Add `.secure(true)` to all cookie builders. If you need to support local development over HTTP, make the Secure flag conditional on the environment (e.g., `cfg!(not(debug_assertions))` or a config setting).

---

### MEDIUM-1: Open Redirect in Logout Endpoint via `id_token_hint` -- FIXED

**Severity:** Medium (FIXED)
**Category:** OWASP A01:2021 — Broken Access Control
**Location:** `fracture-core/src/controllers/oidc.rs` (lines 221-238)

**Description:**
The logout endpoint constructs a redirect URL by appending query parameters to the IdP's `end_session_endpoint`. The `id_token_hint` value is taken directly from the `id_token` cookie and appended to the URL without URL-encoding:

```rust
url.push_str("?id_token_hint=");
url.push_str(hint.value());
```

If an attacker can manipulate the `id_token` cookie value (e.g., it was set without the `Secure` flag — see HIGH-2), they could inject additional query parameters (e.g., `&post_logout_redirect_uri=https://evil.com`) or fragment identifiers to redirect the user to a malicious site after logout.

Similarly, `post_logout_redirect_uri` is appended without URL-encoding:
```rust
url.push_str("post_logout_redirect_uri=");
url.push_str(&oidc.post_logout_redirect_uri);
```

While `post_logout_redirect_uri` comes from config (trusted), the `id_token_hint` from the cookie is user-controllable.

**Recommendation:**
URL-encode the `id_token_hint` and `post_logout_redirect_uri` values before appending them to the URL. Use `urlencoding::encode()` or `form_urlencoded::Serializer`.

---

### MEDIUM-2: No Rate Limiting on Authentication Endpoints

**Severity:** Medium
**Category:** OWASP A07:2021 — Identification and Authentication Failures
**Location:** All `/api/auth/oidc/*` endpoints

**Description:**
There is no rate limiting on any endpoint in the application. The following endpoints are particularly sensitive:

- `GET /api/auth/oidc/authorize` — initiates OIDC flow, allocates server-side state (memory for PKCE verifier + nonce per request)
- `GET /api/auth/oidc/callback` — processes auth code exchange
- `GET /api/auth/oidc/refresh` — issues new JWT tokens
- `POST /api/auth/oidc/backchannel-logout` — processes logout tokens
- `POST /orgs/:pid/members/invite` — sends invitation emails

Without rate limiting, an attacker can:
1. Exhaust server memory by flooding `/authorize` (each request stores a `PendingAuth` in the in-memory `HashMap`)
2. Abuse the invite endpoint for email spam
3. Attempt brute-force attacks on CSRF state values (though the 5-min TTL and random tokens make this impractical)

**Recommendation:**
Add rate limiting middleware. Options:
- Use `tower::limit::RateLimitLayer` or `governor` crate for per-IP rate limiting
- Apply stricter limits to auth and invite endpoints
- Add a maximum size cap to the `OidcStateStore` HashMap

---

### MEDIUM-3: In-Memory OIDC State Store Has No Size Limit

**Severity:** Medium
**Category:** OWASP A04:2021 — Insecure Design
**Location:** `fracture-core/src/controllers/oidc_state.rs`

**Description:**
The `OidcStateStore` uses an unbounded `HashMap<String, PendingAuth>` to store PKCE verifiers and nonces. While expired entries are evicted on each new insert, an attacker could generate thousands of `/authorize` requests within the 5-minute TTL window, growing the HashMap without bound.

Each `PendingAuth` entry contains a PKCE verifier string, nonce string, and a timestamp. At scale, this is a denial-of-service vector.

**Recommendation:**
- Add a maximum capacity to the store (e.g., 10,000 entries). Reject new authorizations when the limit is reached.
- Consider using an LRU cache with a max size.
- This is also mitigated by rate limiting (see MEDIUM-2).

---

### MEDIUM-4: Missing `Strict-Transport-Security` (HSTS) Header -- FIXED

**Severity:** Medium (FIXED)
**Category:** OWASP A05:2021 — Security Misconfiguration
**Location:** `fracture-core/src/initializers/security_headers.rs`

**Description:**
The security headers initializer sets CSP, X-Content-Type-Options, X-Frame-Options, Referrer-Policy, and X-Permitted-Cross-Domain-Policies, but does NOT set `Strict-Transport-Security`. Without HSTS, browsers will not enforce HTTPS-only access, allowing SSL-stripping attacks on first visit.

**Recommendation:**
Add the HSTS header in `security_headers.rs`:
```rust
headers.insert(
    axum::http::header::STRICT_TRANSPORT_SECURITY,
    "max-age=63072000; includeSubDomains; preload"
        .parse()
        .expect("valid header value"),
);
```
Optionally make this conditional on production environment to avoid breaking local HTTP development.

---

### MEDIUM-5: Admin Can Escalate Own Role or Demote Owners -- FIXED

**Severity:** Medium (FIXED)
**Category:** OWASP A01:2021 — Broken Access Control
**Location:** `fracture-core/src/controllers/org.rs` (lines 278-310)

**Description:**
The `update_role` endpoint requires `OrgRole::Admin` to change roles. However, the form in the members template (`members.html:94-99`) allows Admins to set roles including "owner". This means:

1. An Admin can promote themselves to Owner by modifying the POST body to `role=owner`, even though the form dropdown is rendered by the template.
2. An Admin can promote any member to Owner.
3. The `update_role` handler does not check whether the requesting user has authority to grant the target role (e.g., only Owners should be able to grant Owner).

The `remove_member` endpoint has the same issue — an Admin can remove other Admins or attempt to remove Owners (though the "last owner" check prevents removing the final owner).

**Recommendation:**
- Require `OrgRole::Owner` to grant or modify the Owner role.
- Prevent Admins from modifying the roles of other Admins (only Owners should be able to).
- Add a check: the acting user's role must be strictly higher than both the target's current role and the new role being assigned.

---

### MEDIUM-6: Last-Owner Demotion via `update_role` -- FIXED

**Severity:** Medium (FIXED)
**Category:** OWASP A01:2021 — Broken Access Control
**Location:** `fracture-core/src/models/org_members.rs` `update_role()`

**Description:**
The `remove_member()` function correctly checks whether the member being removed is the last owner of the organization, but `update_role()` had no such check. An Admin (or an Owner demoting themselves) could change the sole Owner's role to a lower role, leaving the organization with no owner. An ownerless organization cannot be administered since settings and member management require at least Admin role, and no one can grant Owner role back.

**Recommendation:**
Add the same last-owner check to `update_role()`: if the current role is Owner and the new role is not Owner, count remaining owners and reject if this is the last one.

**Fix applied:** Added owner count check in `update_role()` that returns `DbErr::Custom("Cannot demote the last owner of an organization")` when the demotion would leave zero owners.

---

### LOW-1: Invite Accept Endpoint Does Not Verify Email Matches User -- FIXED

**Severity:** Low (FIXED)
**Category:** OWASP A01:2021 — Broken Access Control
**Location:** `fracture-core/src/controllers/org.rs` (lines 398-415), `fracture-core/src/models/org_invites.rs` (lines 89-116)

**Description:**
The `accept_invite` endpoint accepts an invite for any authenticated user who possesses the invite token (UUID). It does not verify that the authenticated user's email matches the email the invite was sent to.

This means if user A obtains the invite link intended for user B (e.g., the link is shared or intercepted), user A can accept the invite and gain membership in the organization with the specified role.

The UUID token provides 122 bits of entropy (effectively unguessable), and invites expire in 7 days, so the practical risk is limited to scenarios where the invite URL is leaked.

**Recommendation:**
Optionally add an email check: verify that the authenticated user's email matches `invite.email` before accepting. This would prevent token reuse by unintended recipients. However, note that the current auto-accept behavior for new OIDC signups (in `find_or_create_from_oidc`) does check by email, so there is already partial protection.

---

### LOW-2: `DELETE` Endpoints Use GET-like XHR Without CSRF Token

**Severity:** Low
**Category:** OWASP A01:2021 — Broken Access Control
**Location:** `assets/static/app.js` (lines 3-18)

**Description:**
The JavaScript delete handler sends `DELETE` requests via XHR. While browsers do not send cross-origin `DELETE` requests without CORS preflight (which would be denied by the default policy), the `data-delete-url` attribute value is read directly from the DOM. If an XSS vulnerability were found, the delete URL could be manipulated.

The `SameSite=Lax` cookie policy provides baseline protection since Lax does not send cookies on cross-site sub-requests.

**Recommendation:**
This is adequately protected by the combination of SameSite=Lax and CORS defaults. For defense-in-depth, consider adding a CSRF token to XHR requests (e.g., via a custom header like `X-CSRF-Token`).

---

### LOW-3: Session Refresh Interval May Be Too Long

**Severity:** Low
**Category:** OWASP A07:2021 — Identification and Authentication Failures
**Location:** `assets/static/app.js` (line 82)

**Description:**
The client-side session refresh runs every 12 minutes (`12 * 60 * 1000`ms). If the JWT expiration is shorter than the refresh interval, users could experience session expiration between refreshes. Conversely, if the JWT has a long expiration (e.g., hours), the refresh is unnecessary.

The session invalidation check in `get_current_user` (middleware.rs:14) only runs when the JWT is validated, so a backchannel logout may not take effect until the next server-side request.

**Recommendation:**
- Align the refresh interval with the JWT expiration time (refresh at ~75% of the expiration window).
- Document the expected JWT expiration in the configuration.

---

### LOW-4: User Entity Serialization Exposes Sensitive Fields -- FIXED

**Severity:** Low (FIXED)
**Category:** OWASP A01:2021 — Broken Access Control
**Location:** `fracture-core/src/models/_entities/users.rs`

**Description:**
The `users::Model` struct derives `Serialize`, which means all fields including `password` (hash), `api_key`, `reset_token`, `magic_link_token`, `email_verification_token`, and `oidc_subject` can be serialized. While the application currently does not serialize user models directly to HTTP responses (it passes specific fields via `base_context`), any future code that accidentally serializes a `users::Model` to JSON would leak sensitive data.

**Recommendation:**
Add `#[serde(skip_serializing)]` to sensitive fields:
- `password`
- `api_key`
- `reset_token`
- `magic_link_token`
- `email_verification_token`
- `oidc_subject`

---

### INFO-1: OIDC Authentication Flow — Well Implemented

**Severity:** Info (Positive Finding)
**Location:** `fracture-core/src/controllers/oidc.rs`, `fracture-core/src/controllers/oidc_state.rs`

**Description:**
The OIDC implementation follows best practices:
- Uses PKCE (`PkceCodeChallenge::new_random_sha256`) to prevent authorization code interception.
- Uses random CSRF state tokens with server-side validation and one-time use (`take()` removes the entry).
- Validates the ID token with nonce verification and proper audience checks.
- Uses a 5-minute TTL on pending auth state with automatic eviction.
- Properly handles the Zitadel project ID in audience verification.

---

### INFO-2: Org-Scoped Data Isolation — Well Implemented

**Severity:** Info (Positive Finding)
**Location:** All model query methods, all controller endpoints

**Description:**
All data queries are properly scoped by `org_id`:
- `projects::Model::find_by_pid_and_org()` filters by both `pid` and `org_id`
- `projects::Model::find_by_org()` filters by `org_id`
- `notes::Model::find_by_pid_and_org()` filters by both `pid` and `org_id`
- `notes::Model::find_by_project_and_org()` filters by both `project_id` and `org_id`
- Every controller endpoint resolves the org via the current user's membership, preventing IDOR attacks.

PIDs are UUIDs (v4), providing 122 bits of entropy, making enumeration infeasible.

---

### INFO-3: Template XSS Protection — Adequate

**Severity:** Info (Positive Finding)
**Location:** All `.html` templates

**Description:**
Tera templates auto-escape HTML output by default. No uses of the `| safe` filter or `{% raw %}` blocks were found in any template. All user-controlled data (org names, project titles, note bodies, user names, emails) is rendered through standard `{{ variable }}` syntax, which is auto-escaped.

The CSP (`script-src 'self'`) provides an additional layer of XSS protection by blocking inline scripts.

---

### INFO-4: SQL Injection Protection — Adequate

**Severity:** Info (Positive Finding)
**Location:** All model query methods

**Description:**
All database queries use SeaORM's query builder with parameterized values (`.eq()`, `.filter()`, etc.). No raw SQL queries were found anywhere in the codebase. This effectively eliminates SQL injection risks.

---

### INFO-5: Back-Channel Logout — Well Implemented

**Severity:** Info (Positive Finding)
**Location:** `fracture-core/src/controllers/oidc.rs` (lines 314-382)

**Description:**
The backchannel logout implementation follows the OpenID Connect Back-Channel Logout specification:
- Fetches the IdP's JWKS and verifies the JWT signature using the correct `kid`.
- Validates issuer and audience claims.
- Requires the `events` claim with the backchannel-logout event URI.
- Rejects tokens containing a `nonce` claim (per spec).
- Sets `session_invalidated_at` on the user, which is checked on every authenticated request via `get_current_user()`.

---

## Recommendations Summary (Priority Order)

1. ~~**HIGH — Add `Secure` flag to all cookies** for production deployments.~~ FIXED
2. **HIGH — Implement CSRF tokens** on all state-changing form endpoints. (Requires architectural decision)
3. ~~**MEDIUM — URL-encode parameters** in the logout redirect URL construction.~~ FIXED
4. **MEDIUM — Add rate limiting** on auth and invite endpoints. (Requires architectural decision)
5. **MEDIUM — Cap the OIDC state store size** to prevent memory exhaustion. (Pairs with rate limiting)
6. ~~**MEDIUM — Add HSTS header** to the security headers initializer.~~ FIXED
7. ~~**MEDIUM — Restrict role escalation** so only Owners can grant Owner role.~~ FIXED
8. ~~**LOW — Verify invite email matches** accepting user's email.~~ FIXED
9. ~~**LOW — Add `#[serde(skip_serializing)]`** to sensitive user model fields.~~ FIXED
10. **LOW — Align JWT refresh interval** with token expiration time.
