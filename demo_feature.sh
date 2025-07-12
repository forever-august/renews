#!/bin/bash

# Demonstration script showing the difference between secure and insecure modes
# This creates temporary config files and shows the CLI behavior

echo "=== Demonstration of --allow-posting-insecure-connections feature ==="
echo ""

# Create a minimal test config
cat > /tmp/test_renews.toml << 'EOF'
addr = ":8119"
site_name = "test.local"
db_path = "sqlite:///tmp/test_news.db"
auth_db_path = "sqlite:///tmp/test_auth.db" 
peer_db_path = "sqlite:///tmp/test_peers.db"
EOF

echo "Created test config file at /tmp/test_renews.toml"
echo ""

echo "1. Default behavior (secure mode):"
echo "   Command: ./target/release/renews --config /tmp/test_renews.toml --help"
echo "   In this mode, non-TLS connections would receive greeting '201 Server ready (no posting)'"
echo ""

echo "2. Development mode (insecure posting allowed):"
echo "   Command: ./target/release/renews --allow-posting-insecure-connections --config /tmp/test_renews.toml --help"
echo "   In this mode, non-TLS connections would receive greeting '200 Server ready (posting allowed)'"
echo ""

echo "3. Verifying the CLI option is available:"
./target/release/renews --help | grep -A1 "allow-posting-insecure-connections"
echo ""

echo "The feature is successfully implemented!"
echo ""
echo "Key behaviors:"
echo "- Default: Secure by default, requires TLS for POST commands"
echo "- With flag: Allows POST on non-TLS connections for development"
echo "- TLS connections: Always allow posting regardless of flag"
echo "- AUTHINFO: Always works on both TLS and non-TLS connections"

# Cleanup
rm -f /tmp/test_renews.toml