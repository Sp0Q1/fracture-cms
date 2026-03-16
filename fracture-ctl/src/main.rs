use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use clap::{Parser, Subcommand};
use std::fs;
use std::io::Read;
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
    /// Update fracture-ctl to the latest version
    Update,
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

fn cmd_init(image: Option<String>, dev: bool) {
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
    ports:
      - "127.0.0.1:${{APP_PORT:-5150}}:5150"
    volumes:
      - app_data:/app/data
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

    eprintln!("Created:");
    eprintln!("  .env.prod          (chmod 600)");
    eprintln!("  compose.prod.yaml");
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
    let status = Command::new("podman")
        .args(["compose", "-f", "compose.prod.yaml", "up", "-d", "app"])
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

fn main() {
    let cli = Cli::parse();

    // Check for updates on non-update commands (non-blocking, 2s timeout)
    if !matches!(cli.command, Commands::Update) {
        check_for_update();
    }

    match cli.command {
        Commands::Init { image, dev } => cmd_init(image, dev),
        Commands::Up => cmd_up(),
        Commands::Down => cmd_down(),
        Commands::Ci => cmd_ci(),
        Commands::Dev { setup } => cmd_dev(setup),
        Commands::Update => cmd_update(),
    }
}
