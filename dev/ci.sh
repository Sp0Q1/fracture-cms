#!/bin/bash
# Run all CI checks locally in podman containers.
# Mirrors .github/workflows/ci.yaml so you can validate before pushing.
set -euo pipefail

SRC="$(cd "$(dirname "$0")/.." && pwd)"
RUST_IMAGE="localhost/fracture-ci:latest"
SEMGREP_IMAGE="docker.io/semgrep/semgrep:latest"
CARGO_CACHE="fracture-ci-cargo"
# Persistent target dir: without it every check did a full cold build of the
# workspace (the registry cache only saves downloads, not compilation).
TARGET_CACHE="fracture-ci-target"

# Build the CI image if it doesn't exist (bundles rust + sqlite + clippy + rustfmt)
if ! podman image exists "$RUST_IMAGE" 2>/dev/null; then
    echo "Building CI image (one-time)..."
    podman build -t fracture-ci -f "$SRC/dev/Dockerfile.ci" "$SRC/dev"
fi

# Warn if there are uncommitted changes — the container mounts the working
# tree, so local CI will pass even if those changes aren't committed.
if [ -d "$SRC/.git" ] && ! git -C "$SRC" diff --quiet 2>/dev/null; then
    echo "⚠  WARNING: uncommitted changes detected — local CI tests your"
    echo "   working tree, not what is committed. CI in GitHub will differ."
    echo ""
fi

# Named volumes for cargo registry + build artifacts (speed up repeat runs)
podman volume exists "$CARGO_CACHE" 2>/dev/null || podman volume create "$CARGO_CACHE" > /dev/null
podman volume exists "$TARGET_CACHE" 2>/dev/null || podman volume create "$TARGET_CACHE" > /dev/null

# Ensure Cargo.lock is up-to-date before read-only CI checks.
# Mounts only Cargo.toml files writable to regenerate the lockfile.
echo "Updating Cargo.lock..."
podman run --rm \
    -v "$SRC:/src" \
    -v "$CARGO_CACHE:/usr/local/cargo/registry" \
    -v "$TARGET_CACHE:/tmp/target" \
    -e CARGO_TARGET_DIR=/tmp/target \
    -w /src \
    "$RUST_IMAGE" \
    cargo generate-lockfile --quiet

passed=0
failed=0
failures=""

run_check() {
    local name="$1"
    shift
    echo ""
    echo "━━━ $name ━━━"
    if "$@"; then
        echo "✓ $name passed"
        passed=$((passed + 1))
    else
        echo "✗ $name FAILED"
        failed=$((failed + 1))
        failures="$failures  - $name\n"
    fi
}

rust_run() {
    podman run --rm \
        -v "$SRC:/src:ro" \
        -v "$CARGO_CACHE:/usr/local/cargo/registry" \
        -v "$TARGET_CACHE:/tmp/target" \
        -e CARGO_TARGET_DIR=/tmp/target \
        -w /src \
        "$RUST_IMAGE" \
        "$@"
}

# --- rustfmt ---
run_check "rustfmt" \
    rust_run cargo fmt --all -- --check

# --- clippy ---
run_check "clippy" \
    rust_run cargo clippy --all-features -- -D warnings -W clippy::pedantic -W clippy::nursery -W rust-2018-idioms

# --- semgrep ---
run_check "semgrep" \
    podman run --rm -v "$SRC:/src:ro" -w /src "$SEMGREP_IMAGE" \
    semgrep scan --config auto --error \
    --exclude-rule python.django.security.django-no-csrf-token.django-no-csrf-token .

# --- tests ---
run_check "test" \
    rust_run sh -c "\
        DATABASE_URL=sqlite:///tmp/fracture-cms_test.sqlite?mode=rwc \
        cargo test --all-features --all"

# --- summary ---
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━"
echo "  $passed passed, $failed failed"
if [ "$failed" -gt 0 ]; then
    echo ""
    echo "  Failures:"
    echo -e "$failures"
    echo "━━━━━━━━━━━━━━━━━━━━━━"
    exit 1
fi
echo "━━━━━━━━━━━━━━━━━━━━━━"
