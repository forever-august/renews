#!/bin/bash

# Simple manual test script for the --allow-posting-insecure-connections feature
# This script tests that the CLI option is properly recognized

echo "Testing --allow-posting-insecure-connections CLI option..."

# Test 1: Check help output includes the option
echo "Test 1: Checking help output..."
if ./target/release/renews --help | grep -q "allow-posting-insecure-connections"; then
    echo "✓ CLI option appears in help output"
else
    echo "✗ CLI option NOT found in help output"
    exit 1
fi

echo "Test 2: Checking that option is parsed (syntax check only)..."
# We can't easily test the full functionality without setting up databases and networking,
# but we can at least verify the option is parsed correctly
if ./target/release/renews --allow-posting-insecure-connections --help > /dev/null 2>&1; then
    echo "✓ CLI option parses correctly"
else
    echo "✗ CLI option fails to parse"
    exit 1
fi

echo "All manual tests passed!"
echo ""
echo "To test the full functionality:"
echo "1. Create a test config file"
echo "2. Start the server with: ./target/release/renews --allow-posting-insecure-connections --config test-config.toml"
echo "3. Connect via telnet on non-TLS port and verify POST command works"