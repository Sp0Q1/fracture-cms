#!/bin/bash
# Compute SHA-384 SRI hashes for static assets and print them in a form
# you can paste into <link integrity=...> / <script integrity=...> tags.
#
# Usage:
#   ./dev/sri.sh                  # hashes every file under assets/static/
#   ./dev/sri.sh assets/static/app.css   # hash one file
set -euo pipefail
SRC="$(cd "$(dirname "$0")/.." && pwd)"

if [ "$#" -gt 0 ]; then
    targets=("$@")
else
    mapfile -t targets < <(find "$SRC/assets/static" -type f \( -name "*.css" -o -name "*.js" \) | sort)
fi

for f in "${targets[@]}"; do
    hash=$(openssl dgst -sha384 -binary "$f" | openssl base64 -A)
    rel="${f#$SRC/}"
    printf '%-40s sha384-%s\n' "$rel" "$hash"
done
