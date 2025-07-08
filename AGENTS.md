# AI Agent Contributing Guide for Renews

This document provides guidance for AI agents contributing to the Renews NNTP server project.

## Overview

Renews is a minimal NNTP (Network News Transfer Protocol) server implemented in Rust. It stores articles in a database and supports configurable newsgroups with optional TLS support.

### Code Structure and Organization

The codebase is organized into several key modules:

- **`src/main.rs`** - Main binary entry point and CLI argument handling
- **`src/lib.rs`** - Library root exposing public APIs
- **`src/server.rs`** - Core NNTP server implementation and connection handling
- **`src/handlers/`** - NNTP command handlers organized by functionality:
  - `article.rs` - Article retrieval commands (ARTICLE, HEAD, BODY, STAT)
  - `auth.rs` - Authentication commands (AUTHINFO)
  - `group.rs` - Group management commands (GROUP, LIST, LISTGROUP)
  - `info.rs` - Information commands (CAPABILITIES, HELP, DATE)
  - `post.rs` - Article posting commands (POST, IHAVE, CHECK, TAKETHIS)
  - `streaming.rs` - Streaming mode support (MODE STREAM)
  - `utils.rs` - Common handler utilities
- **`src/storage/`** - Database abstraction layer with SQLite and PostgreSQL support
- **`src/auth/`** - Authentication provider implementations
- **`src/config.rs`** - Configuration file parsing and validation
- **`src/parse.rs`** - NNTP protocol parsing utilities
- **`src/responses.rs`** - NNTP response constants and formatting
- **`src/control.rs`** - Control message handling (newgroup, rmgroup, cancel)
- **`src/peers.rs`** - Peer synchronization for distributed newsgroups
- **`src/retention.rs`** - Article retention and cleanup policies
- **`src/wildmat.rs`** - Wildcard pattern matching for newsgroup names
- **`src/ws.rs`** - WebSocket bridge (optional feature)

### Test Organization

Tests are comprehensive and well-organized:

- **`tests/unit/`** - Unit tests for individual modules
- **`tests/integration/`** - Integration tests for full feature workflows
- **`tests/compliance.rs`** - RFC 3977 compliance tests
- **`tests/utils.rs`** - Test utilities and mock implementations

## Building and Testing

### Prerequisites

- Rust toolchain (latest stable)
- SQLite development libraries (for default features)
- PostgreSQL development libraries (for postgres feature)

### Build Instructions

```bash
# Standard build
cargo build

# Release build (optimized)
cargo build --release

# Build with specific features
cargo build --features websocket,postgres
```

### Testing

```bash
# Run all tests
cargo test

# Run only unit tests
cargo test --lib

# Run only integration tests
cargo test --test integration

# Run compliance tests
cargo test --test compliance

# Run specific test
cargo test test_name
```

### Development Tools

The project uses standard Rust development tools:

```bash
# Check for compilation errors without building
cargo check

# Run linter (required before committing)
cargo clippy

# Format code (required before committing)
cargo fmt
```

## Code Quality Guidelines

### Linting and Warnings

- **Always run `cargo clippy`** before submitting changes
- **Fix all clippy warnings** - zero warnings policy
- **Run `cargo clippy --fix`** to automatically apply fixes where possible
- Currently there is one known warning in `src/peers.rs:480` that should be fixed

### Code Formatting

- **Always run `cargo fmt`** before committing
- The project uses standard rustfmt configuration
- **Never commit unformatted code** - formatting is enforced

### Compilation Warnings

- **Fix all compiler warnings** during builds
- Use `#[allow(warning_type)]` only in exceptional cases with justification
- Prefer fixing the underlying issue rather than suppressing warnings

### Documentation Standards

- **Provide useful but not excessive documentation**
- Document public APIs and complex algorithms
- Use `///` for documentation comments on public items
- Use `//` for implementation comments
- Include examples in documentation where helpful
- Keep comments concise and focused on the "why" rather than "what"

Example of good documentation:
```rust
/// Parse a single NNTP command line as described in RFC 3977.
/// 
/// Returns the command name and arguments, handling proper escaping
/// and whitespace according to the protocol specification.
pub fn parse_command(input: &str) -> Result<Command, ParseError> {
    // Implementation details...
}
```

### Error Handling

- Use proper error types and propagation
- Avoid `unwrap()` in library code except in tests
- Prefer `?` operator for error propagation
- Use `Result<T, E>` consistently for fallible operations

## Relevant RFC Documents

The following RFCs are directly relevant to this project:

### Core NNTP Protocol
- **[RFC 3977 - Network News Transfer Protocol (NNTP)](https://tools.ietf.org/rfc/rfc3977.txt)** - Primary NNTP specification
- **[RFC 4643 - Network News Transfer Protocol (NNTP) Extension for Authentication](https://tools.ietf.org/rfc/rfc4643.txt)** - AUTHINFO authentication
- **[RFC 4644 - Network News Transfer Protocol (NNTP) Extension for Streaming Feeds](https://tools.ietf.org/rfc/rfc4644.txt)** - Streaming mode (CHECK/TAKETHIS)

### Message Format and Handling
- **[RFC 2822 - Internet Message Format](https://tools.ietf.org/rfc/rfc2822.txt)** - Message header format and parsing
- **[RFC 3339 - Date and Time on the Internet: Timestamps](https://tools.ietf.org/rfc/rfc3339.txt)** - Date format handling
- **[RFC 5536 - Netnews Article Format](https://tools.ietf.org/rfc/rfc5536.txt)** - Article format specifications
- **[RFC 5537 - Netnews Architecture and Protocols](https://tools.ietf.org/rfc/rfc5537.txt)** - Overall Netnews architecture

### Security and Control Messages
- **[RFC 3981 - NNTP Control Messages](https://tools.ietf.org/rfc/rfc3981.txt)** - Control message handling
- **[RFC 4642 - Using Transport Layer Security (TLS) with Network News Transfer Protocol (NNTP)](https://tools.ietf.org/rfc/rfc4642.txt)** - TLS support

## Development Workflow

1. **Before making changes:**
   - Run `cargo test` to ensure current functionality works
   - Run `cargo clippy` to check for existing warnings
   - Run `cargo fmt --check` to verify formatting

2. **During development:**
   - Write tests for new functionality
   - Add appropriate documentation
   - Follow RFC specifications for protocol compliance

3. **Before committing:**
   - Run `cargo clippy` and fix all warnings
   - Run `cargo fmt` to format code
   - Run `cargo test` to ensure all tests pass
   - Verify build works: `cargo build --release`

4. **Testing guidelines:**
   - Add unit tests for new modules/functions
   - Add integration tests for new features
   - Add compliance tests for NNTP protocol changes
   - Ensure test coverage for error conditions

## Configuration and Features

The project supports multiple build configurations:

- **Default features:** SQLite storage, basic NNTP server
- **`websocket` feature:** WebSocket bridge for web clients
- **`postgres` feature:** PostgreSQL storage backend

When contributing, consider the impact on all supported configurations and test accordingly.