#!/bin/bash
# Run all CI checks locally in podman containers.
# Mirrors .github/workflows/ci.yaml so you can validate before pushing.
set -euo pipefail

SRC="$(cd "$(dirname "$0")/.." && pwd)"
RUST_IMAGE="docker.io/library/rust:latest"
SEMGREP_IMAGE="docker.io/semgrep/semgrep:latest"
CARGO_CACHE="fracture-ci-cargo"

# Named volume for cargo registry cache (speeds up repeat runs)
podman volume exists "$CARGO_CACHE" 2>/dev/null || podman volume create "$CARGO_CACHE" > /dev/null

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
        -e CARGO_TARGET_DIR=/tmp/target \
        -w /src \
        "$RUST_IMAGE" \
        "$@"
}

# --- rustfmt ---
run_check "rustfmt" \
    rust_run sh -c "rustup component add rustfmt > /dev/null 2>&1 && cargo fmt --all -- --check"

# --- clippy ---
run_check "clippy" \
    rust_run sh -c "\
        rustup component add clippy > /dev/null 2>&1 && \
        apt-get update -qq > /dev/null 2>&1 && \
        apt-get install -y -qq libsqlite3-dev > /dev/null 2>&1 && \
        cargo clippy --all-features -- -D warnings -W clippy::pedantic -W clippy::nursery -W rust-2018-idioms"

# --- semgrep ---
run_check "semgrep" \
    podman run --rm -v "$SRC:/src:ro" -w /src "$SEMGREP_IMAGE" \
    semgrep scan --config auto --error \
    --exclude-rule python.django.security.django-no-csrf-token.django-no-csrf-token .

# --- tests ---
run_check "test" \
    rust_run sh -c "\
        apt-get update -qq > /dev/null 2>&1 && \
        apt-get install -y -qq libsqlite3-dev > /dev/null 2>&1 && \
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
