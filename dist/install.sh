#!/bin/bash
set -e

# Installation script for renews tarball distribution
# This script installs renews and its files to the appropriate system locations

if [ "$EUID" -ne 0 ]; then
    echo "Please run this script as root (using sudo)"
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Installing renews NNTP server..."

# Install binary
cp "$SCRIPT_DIR/usr/bin/renews" /usr/bin/renews
chmod 755 /usr/bin/renews

# Install man page
mkdir -p /usr/share/man/man1
cp "$SCRIPT_DIR/usr/share/man/man1/renews.1" /usr/share/man/man1/renews.1
chmod 644 /usr/share/man/man1/renews.1

# Install systemd service
mkdir -p /lib/systemd/system
cp "$SCRIPT_DIR/lib/systemd/system/renews.service" /lib/systemd/system/renews.service
chmod 644 /lib/systemd/system/renews.service

# Install config example
mkdir -p /etc/renews
cp "$SCRIPT_DIR/etc/renews/config.toml.example" /etc/renews/config.toml.example
chmod 644 /etc/renews/config.toml.example

# Create renews user if it doesn't exist
if ! id "renews" &>/dev/null; then
    useradd --system --no-create-home --shell /bin/false renews
    echo "Created renews system user"
fi

# Create data directories
mkdir -p /var/lib/renews
chown renews:renews /var/lib/renews

# Reload systemd
systemctl daemon-reload

echo "Installation complete!"
echo ""
echo "To get started:"
echo "1. Copy /etc/renews/config.toml.example to /etc/renews/config.toml"
echo "2. Edit the configuration file as needed"
echo "3. Initialize the database: renews --config /etc/renews/config.toml --init"
echo "4. Start the service: systemctl start renews"
echo "5. Enable auto-start: systemctl enable renews"
echo ""
echo "For more information, see: man renews"