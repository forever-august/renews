#!/bin/bash
set -e

# Uninstallation script for renews tarball distribution
# This script removes renews and its files from the system

if [ "$EUID" -ne 0 ]; then
    echo "Please run this script as root (using sudo)"
    exit 1
fi

echo "Uninstalling renews NNTP server..."

# Stop and disable service if running
systemctl stop renews 2>/dev/null || true
systemctl disable renews 2>/dev/null || true

# Remove binary
rm -f /usr/bin/renews

# Remove man page
rm -f /usr/share/man/man1/renews.1

# Remove systemd service
rm -f /lib/systemd/system/renews.service

# Remove config (but keep user data)
rm -f /etc/renews/config.toml.example
rmdir /etc/renews 2>/dev/null || echo "Note: /etc/renews directory kept (may contain user config)"

# Reload systemd
systemctl daemon-reload

echo "Uninstallation complete!"
echo ""
echo "Note: User data in /var/renews and /opt/renews was preserved."
echo "The 'renews' system user was also preserved."
echo "To completely remove all data, run:"
echo "  rm -rf /var/renews /opt/renews"
echo "  userdel renews"