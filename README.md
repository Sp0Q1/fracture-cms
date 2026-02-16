# Fracture CMS

A content management system built with [Rust](https://www.rust-lang.org/) on the [Loco](https://loco.rs) framework, featuring OIDC single sign-on and movie CRUD management.

## Features

- **JWT Authentication** — token-based API authentication
- **OIDC via Kanidm** — single sign-on through OpenID Connect
- **Email Verification** — user registration with email confirmation
- **Movie Management** — full CRUD for movie entries with server-side rendered views

## Quick Start

### Local development

```sh
cargo loco start
```

Visit [http://localhost:5150](http://localhost:5150).

### Full stack (with Kanidm, Postgres, Mailcrab)

```sh
./dev/setup.sh
podman compose up
```

See the `dev/` directory for environment configuration and service setup.
