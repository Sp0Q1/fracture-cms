# Production Deployment

This guide covers deploying Fracture CMS to a production environment.

## Prerequisites

- A container runtime (Podman or Docker)
- An OIDC identity provider (Zitadel, Keycloak, Auth0, Google, etc.)
- An SMTP server for invitation emails
- A reverse proxy with TLS (Caddy, nginx, Traefik, etc.)

## 1. Build the Production Image

```bash
podman build -f Containerfile.prod -t fracture-cms:latest .
```

This creates a minimal Debian-based image (~100MB) with only the compiled binary, config, and static assets. No Rust toolchain or source code is included.

## 2. Configure the OIDC Provider

Register Fracture CMS as an OIDC application in your identity provider:

| Setting | Value |
|---------|-------|
| Application type | Web |
| Auth method | Client secret (Basic) |
| Grant type | Authorization code |
| Redirect URI | `https://your-domain.com/api/auth/oidc/callback` |
| Post-logout redirect URI | `https://your-domain.com` |
| Back-channel logout URI | `https://your-domain.com/api/auth/oidc/backchannel-logout` |
| Scopes | `openid`, `email`, `profile` |

Note down the **client ID**, **client secret**, and **issuer URL**.

## 3. Environment Variables

Create an `.env.prod` file (never commit this):

```bash
# Required — JWT session signing key (generate with: openssl rand -base64 32)
JWT_SECRET=<random-32-byte-base64-string>

# Required — OIDC provider settings
OIDC_PROVIDER_NAME=zitadel          # or keycloak, auth0, google, etc.
OIDC_ISSUER_URL=https://auth.your-domain.com
OIDC_CLIENT_ID=<your-client-id>
OIDC_CLIENT_SECRET=<your-client-secret>
OIDC_PROJECT_ID=                     # Zitadel only — leave empty for other providers
OIDC_REDIRECT_URI=https://your-domain.com/api/auth/oidc/callback
OIDC_POST_LOGOUT_REDIRECT_URI=https://your-domain.com

# Required — public URL of the app
APP_URL=https://your-domain.com

# Required — SMTP for invitation emails
MAILER_HOST=smtp.your-provider.com
MAILER_PORT=587
MAILER_USER=apikey
MAILER_PASSWORD=<smtp-password>

# Optional — override defaults
# PORT=5150
# SERVER_BINDING=0.0.0.0
# DATABASE_URL=sqlite:///app/data/fracture-cms.sqlite?mode=rwc
# DB_MAX_CONNECTIONS=5
```

### Generating a JWT secret

```bash
openssl rand -base64 32
```

## 4. Run the Container

```bash
podman run -d \
    --name fracture-cms \
    --env-file .env.prod \
    -p 127.0.0.1:5150:5150 \
    -v fracture-data:/app/data \
    --restart unless-stopped \
    fracture-cms:latest
```

Key points:
- Bind to `127.0.0.1:5150` — the reverse proxy handles public traffic
- Mount a named volume (`fracture-data`) for the SQLite database
- The container runs as a non-root user (`appuser`)
- Migrations run automatically on startup (`auto_migrate: true`)

## 5. Reverse Proxy (TLS)

Fracture CMS does not handle TLS. Put a reverse proxy in front of it.

### Caddy (recommended — automatic HTTPS)

```
your-domain.com {
    reverse_proxy localhost:5150
}
```

### nginx

```nginx
server {
    listen 443 ssl http2;
    server_name your-domain.com;

    ssl_certificate     /etc/letsencrypt/live/your-domain.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/your-domain.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:5150;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}

server {
    listen 80;
    server_name your-domain.com;
    return 301 https://$host$request_uri;
}
```

## 6. Database

Fracture CMS uses SQLite by default. The database file is stored in the `/app/data` volume.

### Backups

Back up the SQLite file regularly:

```bash
# Copy from the named volume
podman run --rm -v fracture-data:/data:ro -v ./backups:/backups \
    alpine cp /data/fracture-cms.sqlite /backups/fracture-cms-$(date +%Y%m%d).sqlite
```

Or use SQLite's `.backup` command for a consistent copy while the app is running:

```bash
podman exec fracture-cms sqlite3 /app/data/fracture-cms.sqlite ".backup '/app/data/backup.sqlite'"
podman cp fracture-cms:/app/data/backup.sqlite ./backups/
```

### Using PostgreSQL instead

SeaORM supports PostgreSQL. To switch:

1. Change `DATABASE_URL` to a PostgreSQL connection string:
   ```
   DATABASE_URL=postgres://user:password@host:5432/fracture_cms
   ```
2. The `sqlx-postgres` feature is already enabled in `Cargo.toml`
3. Migrations are written in SeaORM's database-agnostic schema builder — they work on both SQLite and PostgreSQL

## 7. Health Checks

The application responds to standard HTTP requests. Use any path to verify the server is running:

```bash
curl -s -o /dev/null -w "%{http_code}" http://localhost:5150/
# Should return 200
```

For container orchestrators:

```yaml
healthcheck:
  test: ["CMD-SHELL", "curl -fs http://localhost:5150/ || exit 1"]
  interval: 30s
  timeout: 5s
  retries: 3
```

## 8. Updating

```bash
# Pull latest code
git pull

# Rebuild
podman build -f Containerfile.prod -t fracture-cms:latest .

# Restart (migrations run automatically)
podman stop fracture-cms && podman rm fracture-cms
podman run -d \
    --name fracture-cms \
    --env-file .env.prod \
    -p 127.0.0.1:5150:5150 \
    -v fracture-data:/app/data \
    --restart unless-stopped \
    fracture-cms:latest
```

Database migrations run automatically on startup. SQLite migrations are non-destructive — they only add tables and columns.

## 9. Security Checklist

- [ ] TLS termination via reverse proxy (HTTPS only)
- [ ] `JWT_SECRET` is randomly generated and unique per deployment
- [ ] `.env.prod` is not committed to version control
- [ ] OIDC redirect URIs match your production domain exactly
- [ ] SQLite database volume has restricted file permissions
- [ ] Container binds to `127.0.0.1`, not `0.0.0.0` (reverse proxy handles public access)
- [ ] Back-channel logout URI is configured in your OIDC provider
- [ ] SMTP credentials use an app-specific password or API key
- [ ] Regular database backups are scheduled

## Example: Full Production Stack with Compose

```yaml
# compose.prod.yaml
services:
  app:
    image: fracture-cms:latest
    env_file: .env.prod
    volumes:
      - fracture-data:/app/data
    restart: unless-stopped

  caddy:
    image: docker.io/library/caddy:latest
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile:ro
      - caddy-data:/data
      - caddy-config:/config
    restart: unless-stopped

volumes:
  fracture-data:
  caddy-data:
  caddy-config:
```

```bash
# Caddyfile
your-domain.com {
    reverse_proxy app:5150
}
```

```bash
podman compose -f compose.prod.yaml up -d
```
