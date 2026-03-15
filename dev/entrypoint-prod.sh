#!/bin/sh
set -e

# Validate data directory exists and is writable
if [ ! -d /app/data ]; then
    echo "FATAL: /app/data does not exist"
    exit 1
fi

if [ ! -w /app/data ]; then
    echo "FATAL: /app/data is not writable by UID $(id -u)."
    echo "  Run with --userns=keep-id or fix directory permissions."
    exit 1
fi

# Find the CLI binary — name varies per downstream project
CLI_BIN=$(find ./target/release -maxdepth 1 -name '*-cli' -type f -executable | head -1)

if [ -z "$CLI_BIN" ]; then
    echo "FATAL: No *-cli binary found in ./target/release/"
    ls -la ./target/release/ 2>/dev/null || true
    exit 1
fi

echo "Starting: $CLI_BIN"
exec "$CLI_BIN" start
