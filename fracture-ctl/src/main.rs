use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use clap::{Parser, Subcommand};
use std::fs;
use std::process::Command;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO: &str = "Sp0Q1/fracture-cms";

#[derive(Parser)]
#[command(name = "fracture-ctl", version = VERSION, about = "Fracture CMS project management")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate production config (.env.prod + compose.prod.yaml)
    Init {
        /// Container image to deploy (e.g. ghcr.io/sp0q1/fracture-pt:latest)
        #[arg(long)]
        image: Option<String>,

        /// Git repo to clone for assets and config (e.g. https://github.com/Sp0Q1/fracture-pt.git)
        #[arg(long)]
        repo: Option<String>,

        /// Generate development config instead of production
        #[arg(long)]
        dev: bool,
    },
    /// Start production services
    Up,
    /// Stop all services
    Down,
    /// Run all CI checks (fmt, clippy, semgrep, tests) — run from project repo
    Ci,
    /// Start the development environment — run from project repo
    Dev {
        /// Run Zitadel OIDC setup before starting
        #[arg(long)]
        setup: bool,
    },
    /// Back up the database to a file
    Backup {
        /// Output file path (default: backup-{timestamp}.sql or .sqlite)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Restore the database from a backup file
    Restore {
        /// Backup file to restore from
        file: String,

        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },
    /// Manage platform administrators
    Admin {
        #[command(subcommand)]
        action: AdminAction,
    },
    /// Update fracture-ctl to the latest version
    Update,
}

#[derive(Subcommand)]
enum AdminAction {
    /// Promote a user to platform admin by email
    Set {
        /// Email address of the user to promote
        email: String,
    },
    /// List all platform admins
    List,
}

fn generate_secret(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::fill(&mut buf[..]);
    BASE64.encode(&buf)
}

fn check_for_update() {
    // Quick non-blocking check — don't slow down normal commands
    let url = format!("https://api.github.com/repos/{REPO}/releases?per_page=5");
    let output = Command::new("curl")
        .args(["-sf", "--max-time", "2", &url])
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let body = String::from_utf8_lossy(&output.stdout);
            // Find latest ctl-v* tag
            for line in body.lines() {
                if let Some(start) = line.find("\"ctl-v") {
                    let rest = &line[start + 1..];
                    if let Some(end) = rest.find('"') {
                        let tag = &rest[..end];
                        let latest = tag.strip_prefix("ctl-v").unwrap_or(tag);
                        if latest != VERSION {
                            eprintln!(
                                "Update available: {VERSION} → {latest}. Run: fracture-ctl update"
                            );
                        }
                        return;
                    }
                }
            }
        }
    }
}

fn cmd_update() {
    eprintln!("Current version: {VERSION}");
    eprintln!("Checking for updates...");

    let url = format!("https://api.github.com/repos/{REPO}/releases?per_page=5");
    let output = Command::new("curl")
        .args(["-sf", &url])
        .output()
        .expect("failed to reach GitHub API");

    if !output.status.success() {
        eprintln!("Error: could not reach GitHub API");
        std::process::exit(1);
    }

    let body = String::from_utf8_lossy(&output.stdout);
    let mut latest_tag = String::new();
    for line in body.lines() {
        if let Some(start) = line.find("\"ctl-v") {
            let rest = &line[start + 1..];
            if let Some(end) = rest.find('"') {
                latest_tag = rest[..end].to_string();
                break;
            }
        }
    }

    if latest_tag.is_empty() {
        eprintln!("Error: no releases found");
        std::process::exit(1);
    }

    let latest_version = latest_tag.strip_prefix("ctl-v").unwrap_or(&latest_tag);
    if latest_version == VERSION {
        eprintln!("Already up to date ({VERSION})");
        return;
    }

    eprintln!("Updating to {latest_version}...");

    // Detect platform
    let arch = if cfg!(target_arch = "x86_64") {
        "amd64"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        eprintln!("Error: unsupported architecture");
        std::process::exit(1);
    };

    let os = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        eprintln!("Error: unsupported OS");
        std::process::exit(1);
    };

    let asset = format!("fracture-ctl-{os}-{arch}.tar.gz");
    let download_url = format!("https://github.com/{REPO}/releases/download/{latest_tag}/{asset}");

    // Download to temp file
    let tmp = "/tmp/fracture-ctl-update.tar.gz";
    let status = Command::new("curl")
        .args(["-sfL", "-o", tmp, &download_url])
        .status()
        .expect("failed to download");

    if !status.success() {
        eprintln!("Error: download failed from {download_url}");
        std::process::exit(1);
    }

    // Find current binary path
    let current_exe = std::env::current_exe().expect("cannot determine executable path");

    // Extract to a temp location first
    let tmp_bin = "/tmp/fracture-ctl-new";
    let status = Command::new("tar")
        .args([
            "xzf",
            tmp,
            "-C",
            "/tmp",
            "--transform",
            "s/fracture-ctl/fracture-ctl-new/",
        ])
        .status();

    // Fallback if --transform isn't supported (macOS)
    if status.is_err() || !status.unwrap().success() {
        let _ = Command::new("tar")
            .args(["xzf", tmp, "-C", "/tmp"])
            .status();
        let _ = fs::rename("/tmp/fracture-ctl", tmp_bin);
    }

    // Replace current binary
    if let Err(e) = fs::copy(tmp_bin, &current_exe) {
        eprintln!(
            "Error: could not replace binary at {}: {e}",
            current_exe.display()
        );
        eprintln!("Try: sudo cp {tmp_bin} {}", current_exe.display());
        std::process::exit(1);
    }

    // Cleanup
    let _ = fs::remove_file(tmp);
    let _ = fs::remove_file(tmp_bin);

    eprintln!("Updated to {latest_version}");
}

fn cmd_init(image: Option<String>, repo: Option<String>, dev: bool) {
    if dev {
        let jwt_secret = generate_secret(32);
        println!(
            r#"# Fracture — Development configuration
# Generated by fracture-ctl init --dev
# OIDC values are filled by ./dev/setup.sh after Zitadel starts.

JWT_SECRET={jwt_secret}
OIDC_PROJECT_ID=
OIDC_CLIENT_ID=
OIDC_CLIENT_SECRET="#
        );
        return;
    }

    let jwt_secret = generate_secret(32);
    let db_password = generate_secret(24);
    let image_name = image.unwrap_or_else(|| "ghcr.io/sp0q1/fracture-cms:latest".to_string());

    let env_content = format!(
        r#"# Production configuration
# Generated by fracture-ctl init

APP_IMAGE={image_name}
JWT_SECRET={jwt_secret}

# Database — SQLite is the default. Uncomment for PostgreSQL:
# APP_DB_USER=fracture
# APP_DB_PASSWORD={db_password}
# DATABASE_URL=postgres://fracture:${{APP_DB_PASSWORD}}@db:5432/fracture

# OIDC — optional. The app serves pages without it; login returns 503.
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
"#
    );

    let compose_content = format!(
        r#"# Production compose — generated by fracture-ctl init
# Start:   podman compose -f compose.prod.yaml up -d app
# With DB: podman compose -f compose.prod.yaml up -d

services:
  db:
    image: docker.io/library/postgres:16-alpine
    restart: unless-stopped
    environment:
      POSTGRES_USER: ${{APP_DB_USER:-fracture}}
      POSTGRES_PASSWORD: ${{APP_DB_PASSWORD:-unused}}
      POSTGRES_DB: ${{APP_DB_NAME:-fracture}}
    volumes:
      - db_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U ${{APP_DB_USER:-fracture}}"]
      interval: 5s
      timeout: 3s
      retries: 5

  app:
    image: ${{APP_IMAGE:-{image_name}}}
    restart: unless-stopped
    dns:
      - 9.9.9.9
      - 149.112.112.112
    ports:
      - "127.0.0.1:${{APP_PORT:-5150}}:5150"
    volumes:
      - app_data:/app/data
      - ./assets:/app/assets:ro
      - ./config:/app/config:ro
    env_file:
      - .env.prod
    environment:
      LOCO_ENV: production
      SERVER_BINDING: 0.0.0.0

volumes:
  db_data:
  app_data:
"#
    );

    if std::path::Path::new(".env.prod").exists() {
        eprintln!("Error: .env.prod already exists. Remove it first to regenerate.");
        std::process::exit(1);
    }

    fs::write(".env.prod", &env_content).expect("failed to write .env.prod");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(".env.prod", fs::Permissions::from_mode(0o600))
            .expect("failed to chmod .env.prod");
    }

    fs::write("compose.prod.yaml", &compose_content).expect("failed to write compose.prod.yaml");

    // Clone repo for assets and config if --repo provided
    if let Some(repo_url) = repo {
        if !std::path::Path::new("assets").exists() {
            eprintln!("Cloning assets from {repo_url}...");
            let status = Command::new("git")
                .args([
                    "clone",
                    "--depth",
                    "1",
                    "--filter=blob:none",
                    "--sparse",
                    &repo_url,
                    ".repo-tmp",
                ])
                .status()
                .expect("failed to run git clone");
            if status.success() {
                let _ = Command::new("git")
                    .args([
                        "-C",
                        ".repo-tmp",
                        "sparse-checkout",
                        "set",
                        "assets",
                        "config",
                    ])
                    .status();
                let _ = fs::rename(".repo-tmp/assets", "assets");
                let _ = fs::rename(".repo-tmp/config", "config");
                let _ = fs::remove_dir_all(".repo-tmp");
                eprintln!("  assets/ and config/ ready.");
            } else {
                eprintln!("Warning: git clone failed. Create assets/ and config/ manually.");
            }
        } else {
            eprintln!("  assets/ already exists, skipping clone.");
        }
    }

    eprintln!("Created:");
    eprintln!("  .env.prod          (chmod 600)");
    eprintln!("  compose.prod.yaml");
    if !std::path::Path::new("assets").exists() {
        eprintln!();
        eprintln!("  Assets not found. Either:");
        eprintln!("    fracture-ctl init --image <image> --repo <git-url>");
        eprintln!("    git clone <repo> && cp -r <repo>/assets <repo>/config .");
    }
    eprintln!();
    eprintln!("Next:");
    eprintln!("  vim .env.prod                                # configure OIDC, SMTP, etc.");
    eprintln!("  fracture-ctl up");
}

fn cmd_up() {
    if !std::path::Path::new("compose.prod.yaml").exists() {
        eprintln!("Error: compose.prod.yaml not found. Run: fracture-ctl init --image <image>");
        std::process::exit(1);
    }
    // Check for aardvark-dns
    if Command::new("which")
        .arg("aardvark-dns")
        .stdout(std::process::Stdio::null())
        .status()
        .map(|s| !s.success())
        .unwrap_or(true)
    {
        eprintln!("WARNING: aardvark-dns is not installed. Containers will not be able to");
        eprintln!("  resolve external hostnames. Install it with: sudo apt install aardvark-dns");
        eprintln!();
    }

    // Pull latest image before starting
    eprintln!("Pulling latest image...");
    let _ = Command::new("podman")
        .args(["compose", "-f", "compose.prod.yaml", "pull", "app"])
        .status();
    // Force recreate to use the new image
    let status = Command::new("podman")
        .args([
            "compose",
            "-f",
            "compose.prod.yaml",
            "up",
            "-d",
            "--force-recreate",
            "app",
        ])
        .status()
        .expect("failed to run podman compose");
    std::process::exit(status.code().unwrap_or(1));
}

fn cmd_down() {
    let compose = if std::path::Path::new("compose.prod.yaml").exists() {
        "compose.prod.yaml"
    } else if std::path::Path::new("compose.yaml").exists() {
        "compose.yaml"
    } else {
        eprintln!("Error: no compose file found");
        std::process::exit(1);
    };
    let status = Command::new("podman")
        .args(["compose", "-f", compose, "down"])
        .status()
        .expect("failed to run podman compose down");
    std::process::exit(status.code().unwrap_or(1));
}

fn cmd_ci() {
    if !std::path::Path::new("dev/ci.sh").exists() {
        eprintln!("Error: dev/ci.sh not found. Run from a project directory.");
        std::process::exit(1);
    }
    let status = Command::new("bash")
        .arg("dev/ci.sh")
        .status()
        .expect("failed to run ci.sh");
    std::process::exit(status.code().unwrap_or(1));
}

fn cmd_dev(setup: bool) {
    let compose = if std::path::Path::new("compose.yaml").exists() {
        "compose.yaml"
    } else {
        eprintln!("Error: compose.yaml not found. Run from a project directory.");
        std::process::exit(1);
    };

    if setup {
        if !std::path::Path::new("dev/setup.sh").exists() {
            eprintln!("Error: dev/setup.sh not found");
            std::process::exit(1);
        }
        let status = Command::new("bash")
            .arg("dev/setup.sh")
            .status()
            .expect("failed to run setup.sh");
        if !status.success() {
            eprintln!("Setup failed");
            std::process::exit(1);
        }
    }

    let status = Command::new("podman")
        .args(["compose", "-f", compose, "up", "-d", "mailcrab", "app"])
        .status()
        .expect("failed to run podman compose");

    if status.success() {
        eprintln!("\nDevelopment environment running:");
        eprintln!("  App:     http://localhost:5150");
        eprintln!("  Zitadel: http://localhost:8080");
        eprintln!("  Mail:    http://localhost:1080");
    }
    std::process::exit(status.code().unwrap_or(1));
}

/// Detect the database type from .env.prod or compose config.
/// Returns ("postgres", db_user, db_password, db_name, db_host) or ("sqlite", path, "", "", "").
fn detect_database() -> (String, String, String, String, String) {
    // Check .env.prod for DATABASE_URL
    let env_content = fs::read_to_string(".env.prod")
        .or_else(|_| fs::read_to_string(".env"))
        .unwrap_or_default();

    for line in env_content.lines() {
        let line = line.trim();
        if line.starts_with('#') || !line.contains('=') {
            continue;
        }
        if let Some(val) = line.strip_prefix("DATABASE_URL=") {
            let val = val.trim();
            if val.starts_with("postgres://") || val.starts_with("postgresql://") {
                // Parse postgres URL: postgres://user:pass@host:port/dbname
                let after_scheme = val
                    .strip_prefix("postgres://")
                    .or_else(|| val.strip_prefix("postgresql://"))
                    .unwrap_or(val);
                let (userpass, hostdb) = after_scheme.split_once('@').unwrap_or(("", after_scheme));
                let (user, pass) = userpass.split_once(':').unwrap_or((userpass, ""));
                let (hostport, dbname) = hostdb.split_once('/').unwrap_or((hostdb, "fracture"));
                let host = hostport.split(':').next().unwrap_or("localhost");
                return (
                    "postgres".into(),
                    user.to_string(),
                    pass.to_string(),
                    dbname.to_string(),
                    host.to_string(),
                );
            }
            if val.starts_with("sqlite") {
                // sqlite:///path/to/db.sqlite?mode=rwc
                let path = val
                    .strip_prefix("sqlite:///")
                    .unwrap_or(val)
                    .split('?')
                    .next()
                    .unwrap_or("data/app.sqlite");
                return (
                    "sqlite".into(),
                    path.to_string(),
                    String::new(),
                    String::new(),
                    String::new(),
                );
            }
        }
    }

    // Default: check for SQLite file in common locations
    for path in &[
        "/app/data/gethacked.sqlite",
        "data/app.sqlite",
        "gethacked.sqlite",
    ] {
        if std::path::Path::new(path).exists() {
            return (
                "sqlite".into(),
                (*path).to_string(),
                String::new(),
                String::new(),
                String::new(),
            );
        }
    }

    // Check if compose has a db service (implies postgres)
    let compose = fs::read_to_string("compose.prod.yaml").unwrap_or_default();
    if compose.contains("postgres") {
        // Read credentials from env vars in compose
        let user = env_content
            .lines()
            .find_map(|l| l.strip_prefix("APP_DB_USER="))
            .unwrap_or("fracture")
            .to_string();
        let pass = env_content
            .lines()
            .find_map(|l| l.strip_prefix("APP_DB_PASSWORD="))
            .unwrap_or("")
            .to_string();
        let name = env_content
            .lines()
            .find_map(|l| l.strip_prefix("APP_DB_NAME="))
            .unwrap_or("fracture")
            .to_string();
        return ("postgres".into(), user, pass, name, "db".into());
    }

    // Fallback: assume SQLite on app_data volume
    (
        "sqlite".into(),
        "/app/data/gethacked.sqlite".into(),
        String::new(),
        String::new(),
        String::new(),
    )
}

fn cmd_backup(output: Option<String>) {
    let (db_type, db_user_or_path, db_pass, db_name, db_host) = detect_database();
    let timestamp = chrono_timestamp();

    match db_type.as_str() {
        "postgres" => {
            let out = output.unwrap_or_else(|| format!("backup-{timestamp}.sql"));
            eprintln!("Backing up PostgreSQL database '{db_name}' on {db_host}...");

            // Use podman exec to run pg_dump inside the db container
            let status = Command::new("podman")
                .args([
                    "compose",
                    "-f",
                    compose_file(),
                    "exec",
                    "-T",
                    "db",
                    "pg_dump",
                    "-U",
                    &db_user_or_path,
                    "-d",
                    &db_name,
                    "--no-owner",
                    "--no-privileges",
                    "--clean",
                    "--if-exists",
                ])
                .stdout(fs::File::create(&out).expect("cannot create output file"))
                .envs(if db_pass.is_empty() {
                    vec![]
                } else {
                    vec![("PGPASSWORD", db_pass.as_str())]
                })
                .status()
                .expect("failed to run pg_dump");

            if status.success() {
                let size = fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
                eprintln!("Backup saved: {out} ({} bytes)", size);
            } else {
                eprintln!("Error: pg_dump failed");
                let _ = fs::remove_file(&out);
                std::process::exit(1);
            }
        }
        "sqlite" => {
            let out = output.unwrap_or_else(|| format!("backup-{timestamp}.sqlite"));
            let cname = container_name("app");
            eprintln!("Backing up SQLite database '{db_user_or_path}' from container '{cname}'...");

            // Checkpoint WAL to ensure consistent copy (best-effort)
            let _ = Command::new("podman")
                .args([
                    "exec",
                    &cname,
                    "sh",
                    "-c",
                    &format!(
                        "sqlite3 '{}' 'PRAGMA wal_checkpoint(TRUNCATE);' 2>/dev/null || true",
                        db_user_or_path
                    ),
                ])
                .status();

            // Use podman cp (not podman-compose cp which doesn't exist)
            let cp_status = Command::new("podman")
                .args(["cp", &format!("{cname}:{db_user_or_path}"), &out])
                .status()
                .expect("failed to run podman cp");

            if !cp_status.success() {
                eprintln!("Error: could not copy database file from container");
                eprintln!("Is the app container running? Check: podman ps");
                std::process::exit(1);
            }

            let size = fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
            eprintln!("Backup saved: {out} ({size} bytes)");
        }
        _ => {
            eprintln!("Error: unknown database type '{db_type}'");
            std::process::exit(1);
        }
    }
}

fn cmd_restore(file: String, yes: bool) {
    if !std::path::Path::new(&file).exists() {
        eprintln!("Error: backup file not found: {file}");
        std::process::exit(1);
    }

    let (db_type, db_user_or_path, db_pass, db_name, _db_host) = detect_database();

    if !yes {
        eprintln!("WARNING: This will replace the current {db_type} database with the backup from '{file}'.");
        eprintln!("         The application should be stopped first (fracture-ctl down).");
        eprint!("Continue? [y/N] ");
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer).unwrap_or(0);
        if answer.trim().to_lowercase() != "y" {
            eprintln!("Aborted.");
            std::process::exit(0);
        }
    }

    match db_type.as_str() {
        "postgres" => {
            eprintln!("Restoring PostgreSQL database '{db_name}' from {file}...");

            // Pipe the SQL file into psql
            let input = fs::File::open(&file).expect("cannot open backup file");
            let status = Command::new("podman")
                .args([
                    "compose",
                    "-f",
                    compose_file(),
                    "exec",
                    "-T",
                    "db",
                    "psql",
                    "-U",
                    &db_user_or_path,
                    "-d",
                    &db_name,
                ])
                .stdin(input)
                .envs(if db_pass.is_empty() {
                    vec![]
                } else {
                    vec![("PGPASSWORD", db_pass.as_str())]
                })
                .status()
                .expect("failed to run psql");

            if status.success() {
                eprintln!("Restore complete. Restart the app: fracture-ctl up");
            } else {
                eprintln!("Error: psql restore failed");
                std::process::exit(1);
            }
        }
        "sqlite" => {
            let cname = container_name("app");
            eprintln!("Restoring SQLite database from {file} into container '{cname}'...");

            // Use podman cp to copy backup into container
            let cp_status = Command::new("podman")
                .args(["cp", &file, &format!("{cname}:{db_user_or_path}")])
                .status()
                .expect("failed to run podman cp");

            if cp_status.success() {
                // Remove WAL/SHM files so the app starts clean
                let _ = Command::new("podman")
                    .args([
                        "exec",
                        &cname,
                        "rm",
                        "-f",
                        &format!("{db_user_or_path}-wal"),
                        &format!("{db_user_or_path}-shm"),
                    ])
                    .status();
                eprintln!("Restore complete. Restart the app: fracture-ctl up");
            } else {
                eprintln!("Error: could not copy backup into container");
                eprintln!("Is the container running? Try: podman ps");
                std::process::exit(1);
            }
        }
        _ => {
            eprintln!("Error: unknown database type '{db_type}'");
            std::process::exit(1);
        }
    }
}

/// Get a simple timestamp string for backup filenames.
fn chrono_timestamp() -> String {
    let output = Command::new("date")
        .args(["+%Y%m%d-%H%M%S"])
        .output()
        .expect("failed to get timestamp");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Resolve the running container name for a compose service.
fn container_name(service: &str) -> String {
    let output = Command::new("podman")
        .args([
            "compose",
            "-f",
            compose_file(),
            "ps",
            "--format",
            "{{.Names}}",
            service,
        ])
        .output();
    if let Ok(out) = output {
        let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !name.is_empty() {
            // podman-compose may return multiple lines; take the first
            return name.lines().next().unwrap_or(&name).to_string();
        }
    }
    // Fallback: guess from common naming conventions
    let dir = std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "fracture".into())
        .replace(['-', '_'], "");
    format!("{dir}_{service}_1")
}

/// Find the compose file to use.
fn compose_file() -> &'static str {
    if std::path::Path::new("compose.prod.yaml").exists() {
        "compose.prod.yaml"
    } else if std::path::Path::new("compose.yaml").exists() {
        "compose.yaml"
    } else {
        eprintln!("Error: no compose file found");
        std::process::exit(1);
    }
}

/// Run a SQL query against the database and return stdout.
/// Uses sqlite3 on the host (via volume path) or psql via podman exec.
fn run_db_query(sql: &str) -> String {
    let (db_type, db_user_or_path, db_pass, db_name, _db_host) = detect_database();

    match db_type.as_str() {
        "sqlite" => {
            // Try to find the SQLite file via the volume
            let volume_path = Command::new("podman")
                .args([
                    "volume",
                    "inspect",
                    "fracture-pt_app_data",
                    "--format",
                    "{{.Mountpoint}}",
                ])
                .output()
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                    } else {
                        None
                    }
                });

            // Also try: list all volumes and find one containing "app_data"
            let volume_path = volume_path.or_else(|| {
                let Ok(o) = Command::new("podman")
                    .args(["volume", "ls", "--format", "{{.Name}}"])
                    .output()
                else {
                    return None;
                };
                if !o.status.success() {
                    return None;
                }
                let names = String::from_utf8_lossy(&o.stdout);
                for name in names.lines() {
                    let name = name.trim();
                    if name.contains("app_data") || name.contains("appdata") {
                        if let Ok(inspect) = Command::new("podman")
                            .args(["volume", "inspect", name, "--format", "{{.Mountpoint}}"])
                            .output()
                        {
                            if inspect.status.success() {
                                return Some(
                                    String::from_utf8_lossy(&inspect.stdout).trim().to_string(),
                                );
                            }
                        }
                    }
                }
                None
            });

            if let Some(vol) = volume_path {
                // Derive the filename from the db path
                let filename = db_user_or_path
                    .rsplit('/')
                    .next()
                    .unwrap_or("gethacked.sqlite");
                let db_path = format!("{vol}/{filename}");

                // Try without sudo first (rootless podman — user owns the volume)
                let output = Command::new("sqlite3").args([&db_path, sql]).output();
                if let Ok(o) = output {
                    if o.status.success() {
                        return String::from_utf8_lossy(&o.stdout).to_string();
                    }
                }

                // Fall back to sudo only if needed
                let output = Command::new("sudo")
                    .args(["sqlite3", &db_path, sql])
                    .output();
                if let Ok(o) = output {
                    if o.status.success() {
                        return String::from_utf8_lossy(&o.stdout).to_string();
                    }
                }
            }

            eprintln!("Error: could not access SQLite database.");
            eprintln!("Install sqlite3: sudo apt install sqlite3");
            std::process::exit(1);
        }
        "postgres" => {
            let cname = container_name("db");
            let output = Command::new("podman")
                .args([
                    "exec",
                    "-i",
                    &cname,
                    "psql",
                    "-U",
                    &db_user_or_path,
                    "-d",
                    &db_name,
                    "-t",
                    "-A",
                    "-c",
                    sql,
                ])
                .envs(if db_pass.is_empty() {
                    vec![]
                } else {
                    vec![("PGPASSWORD", db_pass.as_str())]
                })
                .output()
                .expect("failed to run psql");
            if output.status.success() {
                return String::from_utf8_lossy(&output.stdout).to_string();
            }
            eprintln!("Error: psql query failed");
            eprintln!("{}", String::from_utf8_lossy(&output.stderr));
            std::process::exit(1);
        }
        _ => {
            eprintln!("Error: unknown database type");
            std::process::exit(1);
        }
    }
}

fn cmd_admin(action: AdminAction) {
    match action {
        AdminAction::Set { email } => {
            eprintln!("Promoting '{email}' to platform admin...");

            // Find the user
            let result = run_db_query(&format!(
                "SELECT id, name FROM users WHERE email = '{}'",
                email.replace('\'', "''")
            ));
            let result = result.trim();
            if result.is_empty() {
                eprintln!("Error: no user found with email '{email}'");
                eprintln!("The user must log in at least once before they can be promoted.");
                std::process::exit(1);
            }
            let parts: Vec<&str> = result.split('|').collect();
            let user_id = parts[0];
            let user_name = parts.get(1).unwrap_or(&"");
            eprintln!("  Found user: {user_name} (id={user_id})");

            // Find the platform admin org
            let org_result = run_db_query(
                "SELECT id, name FROM organizations WHERE is_platform_admin = 1 LIMIT 1",
            );
            let org_result = org_result.trim();
            if org_result.is_empty() {
                eprintln!("Error: no platform admin organization found.");
                eprintln!(
                    "The app may not have been seeded. Start it once to create the admin org."
                );
                std::process::exit(1);
            }
            let org_parts: Vec<&str> = org_result.split('|').collect();
            let org_id = org_parts[0];
            let org_name = org_parts.get(1).unwrap_or(&"");
            eprintln!("  Admin org: {org_name} (id={org_id})");

            // Check if already a member
            let existing = run_db_query(&format!(
                "SELECT id FROM org_members WHERE org_id = {org_id} AND user_id = {user_id}"
            ));
            if !existing.trim().is_empty() {
                eprintln!("  User is already a member of the admin org. Updating role to owner...");
                run_db_query(&format!(
                    "UPDATE org_members SET role = 'owner' WHERE org_id = {org_id} AND user_id = {user_id}"
                ));
            } else {
                run_db_query(&format!(
                    "INSERT INTO org_members (org_id, user_id, role, created_at, updated_at) \
                     VALUES ({org_id}, {user_id}, 'owner', datetime('now'), datetime('now'))"
                ));
            }

            eprintln!("  Done! {user_name} is now a platform admin.");
            eprintln!("  Refresh the browser to see the Admin menu.");
        }
        AdminAction::List => {
            let result = run_db_query(
                "SELECT u.email, u.name, om.role \
                 FROM users u \
                 JOIN org_members om ON om.user_id = u.id \
                 JOIN organizations o ON o.id = om.org_id \
                 WHERE o.is_platform_admin = 1 \
                 ORDER BY om.role, u.name",
            );
            let result = result.trim();
            if result.is_empty() {
                eprintln!("No platform admins found.");
                eprintln!("Promote one with: fracture-ctl admin set user@example.com");
            } else {
                eprintln!("Platform admins:");
                for line in result.lines() {
                    let parts: Vec<&str> = line.split('|').collect();
                    let email = parts.first().unwrap_or(&"?");
                    let name = parts.get(1).unwrap_or(&"?");
                    let role = parts.get(2).unwrap_or(&"?");
                    eprintln!("  {name} <{email}> ({role})");
                }
            }
        }
    }
}

fn main() {
    let cli = Cli::parse();

    // Check for updates on non-update commands (non-blocking, 2s timeout)
    if !matches!(cli.command, Commands::Update) {
        check_for_update();
    }

    match cli.command {
        Commands::Init { image, repo, dev } => cmd_init(image, repo, dev),
        Commands::Up => cmd_up(),
        Commands::Down => cmd_down(),
        Commands::Ci => cmd_ci(),
        Commands::Dev { setup } => cmd_dev(setup),
        Commands::Backup { output } => cmd_backup(output),
        Commands::Restore { file, yes } => cmd_restore(file, yes),
        Commands::Admin { action } => cmd_admin(action),
        Commands::Update => cmd_update(),
    }
}
