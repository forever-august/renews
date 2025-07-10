# Configuration Guide

This document provides comprehensive configuration guidance for Renews NNTP server.

## Configuration File Format

Renews uses TOML format for configuration. The default location is `/etc/renews.toml`, but can be specified with `--config` or the `RENEWS_CONFIG` environment variable.

## Basic Configuration

### Minimal Setup

```toml
# Basic server settings
addr = ":119"                    # Listen address for NNTP
site_name = "news.example.com"   # Server hostname
db_path = "sqlite:///var/renews/news.db"     # Article database
auth_db_path = "sqlite:///var/renews/auth.db" # User database
```

### Complete Example

```toml
# Network settings
addr = ":119"                    # NNTP listen address
site_name = "news.example.com"   # Server hostname

# Database paths
db_path = "sqlite:///var/renews/news.db"
auth_db_path = "sqlite:///var/renews/auth.db"  
peer_db_path = "sqlite:///var/renews/peers.db"

# Connection settings
idle_timeout_secs = 600          # Client idle timeout (10 minutes)
peer_sync_secs = 3600           # Default peer sync interval (1 hour)

# TLS configuration (optional)
tls_addr = ":563"               # NNTPS listen address
tls_cert = "/etc/ssl/news.crt"  # TLS certificate file
tls_key = "/etc/ssl/news.key"   # TLS private key file

# WebSocket bridge (optional, requires websocket feature)
ws_addr = ":8080"               # WebSocket listen address

# Article retention defaults
default_retention_days = 30     # Keep articles for 30 days
default_max_article_bytes = "1M" # 1 megabyte article limit

# Per-group settings
[[group_settings]]
pattern = "announce.*"          # Groups matching this pattern
retention_days = 90             # Keep announcements longer
max_article_bytes = "500K"      # Smaller size limit

[[group_settings]]  
group = "alt.binaries.test"     # Specific group
retention_days = 7              # Short retention for test group
max_article_bytes = "10M"       # Larger files allowed

# Peer servers for synchronization
[[peers]]
sitename = "user:pass@peer1.example.com:119"
patterns = ["comp.*", "misc.*"] # Only sync these hierarchies
sync_schedule = "0 */30 * * * *"       # Sync every 30 minutes

[[peers]]
sitename = "peer2.example.com"  # No authentication required
patterns = ["*"]                # Sync all groups
sync_schedule = "0 0 */2 * * *"       # Sync every 2 hours
```

## Configuration Sections

### Network Settings

| Setting | Description | Default |
|---------|-------------|---------|
| `addr` | NNTP listen address | Required |
| `site_name` | Server hostname | `$HOSTNAME` or `localhost` |
| `tls_addr` | NNTPS listen address | None |
| `ws_addr` | WebSocket listen address | None |
| `idle_timeout_secs` | Client connection timeout | 600 |

### Database Settings

| Setting | Description | Default |
|---------|-------------|---------|
| `db_path` | Article database URI | `sqlite:///var/renews/news.db` |
| `auth_db_path` | Authentication database URI | `sqlite:///var/renews/auth.db` |
| `peer_db_path` | Peer state database URI | `sqlite:///var/renews/peers.db` |

#### Database URI Formats

**SQLite:**
```toml
db_path = "sqlite:///absolute/path/to/database.db"
db_path = "sqlite://relative/path/to/database.db"  
```

**PostgreSQL:**
```toml
db_path = "postgres://user:password@localhost/renews"
db_path = "postgres://user@localhost/renews"  # No password
```

### TLS Configuration

All three settings must be provided to enable TLS:

```toml
tls_addr = ":563"                    # Standard NNTPS port
tls_cert = "/path/to/certificate.pem" # PEM format certificate
tls_key = "/path/to/private.key"      # PEM format private key
```

### Article Retention

Global defaults:
```toml
default_retention_days = 30      # Days to keep articles
default_max_article_bytes = "1M" # Maximum article size
```

Size format supports suffixes: `K` (kilobytes), `M` (megabytes), `G` (gigabytes).

### Group-Specific Rules

Override defaults for specific groups or patterns:

```toml
[[group_settings]]
group = "alt.test"              # Exact group match
retention_days = 7
max_article_bytes = "500K"

[[group_settings]]
pattern = "comp.lang.*"         # Wildcard pattern
retention_days = 90
```

Pattern matching uses wildmat syntax:
- `*` matches any string
- `?` matches any single character  
- `[abc]` matches any character in brackets
- `[!abc]` matches any character not in brackets

### Peer Synchronization

Configure peer servers for article distribution:

```toml
[[peers]]
sitename = "news.example.com:119"    # Hostname and port
patterns = ["*"]                     # Groups to sync (wildmat)
sync_schedule = "0 0 * * * *"           # Override default schedule

[[peers]]
sitename = "user:pass@secure.example.com:563"  # With credentials
patterns = ["comp.*", "!comp.sys.mac.*"]       # Include/exclude patterns
```

#### Peer Patterns

- `["*"]` - Sync all groups
- `["comp.*"]` - Only comp.* hierarchy  
- `["comp.*", "misc.*"]` - Multiple hierarchies
- `["*", "!alt.*"]` - All except alt.* groups

## Variable Substitution

Configuration supports environment variables and file inclusion:

### Environment Variables

```toml
site_name = "$ENV{HOSTNAME}"
db_path = "sqlite:///$ENV{DATA_DIR}/news.db"
```

### File Inclusion

```toml
tls_cert = "$FILE{/etc/ssl/certs/news.crt}"
```

This replaces the value with the contents of the specified file.

## PostgreSQL Backend

To use PostgreSQL instead of SQLite:

1. Build with PostgreSQL support:
   ```bash
   cargo build --release --features postgres
   ```

2. Configure database URI:
   ```toml
   db_path = "postgres://renews:password@localhost/renews"
   auth_db_path = "postgres://renews:password@localhost/renews_auth"
   ```

3. Ensure PostgreSQL server is running and databases exist.

## WebSocket Bridge

For web-based NNTP clients:

1. Build with WebSocket support:
   ```bash
   cargo build --release --features websocket
   ```

2. Configure WebSocket address:
   ```toml
   ws_addr = ":8080"
   ```

Web clients can connect via WebSocket and use NNTP protocol over the connection.

## Runtime Configuration Reload

Send `SIGHUP` to reload configuration:

```bash
systemctl reload renews
# or
kill -HUP $(pidof renews)
```

**Reloadable settings:**
- Retention policies
- Group settings  
- TLS certificates
- Peer configurations

**Non-reloadable settings:**
- Listen addresses
- Database paths
- WebSocket settings

## Configuration Validation

Test configuration without starting server:

```bash
renews --config /path/to/config.toml --check
```

Initialize databases:

```bash
renews --config /path/to/config.toml --init
```