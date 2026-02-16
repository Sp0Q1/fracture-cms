#!/bin/sh
set -e

# Install Kanidm CA certificate if available
if [ -f /certs/ca.pem ]; then
    cp /certs/ca.pem /usr/local/share/ca-certificates/kanidm.crt
    update-ca-certificates
fi

exec ./target/release/fracture_cms-cli start
