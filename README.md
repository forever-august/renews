# Renews

Renews is a minimal NNTP server implemented in Rust.  It stores articles in a
database and supports a configurable set of newsgroups.  The server can
optionally accept NNTP over TLS when the TLS parameters are provided.

## Building

```bash
cargo build --release
```
This produces the `renews` binary in `target/release/`.

## Configuration

Configuration is loaded from the file specified with `--config`. When the
`RENEWS_CONFIG` environment variable is set it is used as the default,
otherwise `/etc/renews.toml` is assumed. The
following keys are recognised:

- `addr` - listen address for plain NNTP connections. If the host portion is
  omitted the server listens on all interfaces.
- `site_name` - hostname advertised by the server. Defaults to the `HOSTNAME`
  environment variable or `localhost` when unset.
- `db_path` - database connection string for storing articles. Defaults to
  `sqlite:///var/renews/news.db`.
- `auth_db_path` - authentication backend connection URI. Defaults to
  `sqlite:///var/renews/auth.db` when unset. The scheme selects the backend,
  for example `sqlite:///path/to/db` or `postgres://host:port`.
- `peer_db_path` - connection string for the peer state database. Defaults to
  `sqlite:///var/renews/peers.db`.
- `peer_sync_secs` - default seconds between synchronizing with peers.
- `peers` - list of peer entries with `sitename`, optional `sync_interval_secs` and `patterns` controlling which groups are exchanged. Each peer may also specify optional `username` and `password` used for `AUTHINFO` when sending articles.
- `tls_addr` - optional listen address for NNTP over TLS. Omitting the host
  portion listens on all interfaces.
- `tls_cert` - path to the TLS certificate in PEM format.
- `tls_key` - path to the TLS private key in PEM format.
- `ws_addr` - optional listen address for the WebSocket bridge (requires the
  `websocket` feature). Omitting the host portion listens on all interfaces.
- `default_retention_days` - default number of days to keep articles.
- `default_max_article_bytes` - default maximum article size in bytes. A `K`,
  `M` or `G` suffix may be used to specify kilobytes, megabytes or gigabytes.
- `group_settings` - list of per-group rules which can match a `group` exactly or a
  `pattern` using wildmat syntax to override retention and size defaults.

An example configuration is provided in the repository:

```toml
addr = ":119"
site_name = "example.com"
db_path = "sqlite:///var/renews/news.db"
auth_db_path = "sqlite:///var/renews/auth.db"
peer_db_path = "sqlite:///var/renews/peers.db"
peer_sync_secs = 3600
tls_addr = ":563"
tls_cert = "cert.pem"
tls_key = "key.pem"
ws_addr = ":8080"
default_retention_days = 30
default_max_article_bytes = "1M"

[[group_settings]]
pattern = "short.*"
retention_days = 7

[[group_settings]]
group = "misc.news"
retention_days = 60
max_article_bytes = "2M"

[[peers]]
sitename = "peer.example.com"
patterns = ["*"]
sync_interval_secs = 3600
username = "peeruser"
password = "peerpass"
```

`tls_addr`, `tls_cert` and `tls_key` must all be set for TLS support to be
enabled. The WebSocket bridge is started when `ws_addr` is set and the crate is
compiled with the `websocket` feature.

## Deployment with systemd

By default the service reads `/etc/renews.toml`. A different path can be
provided with `--config`. A simple systemd unit may look like this:

```ini
[Unit]
Description=Renews NNTP server
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/renews --config /opt/renews/config.toml
ExecReload=/bin/kill -HUP $MAINPID
WorkingDirectory=/opt/renews
Restart=on-failure
User=renews
Group=renews

[Install]
WantedBy=multi-user.target
```

Install the file as `/etc/systemd/system/renews.service` and run
`systemctl enable --now renews` to start the server at boot.

Sending `SIGHUP` to the process (for example with `systemctl reload`) reloads
the configuration. Retention, group, and TLS settings are updated at runtime;
the listening ports and database paths remain unchanged.


## Administration

Use the `admin` subcommand to manage newsgroups and users without starting the
server. These commands read the same configuration file as the server itself.

```bash
# configuration is supplied via the environment
export RENEWS_CONFIG=/opt/renews/config.toml

# add a newsgroup
renews admin add-group rust.news --moderated

# remove a user
renews admin remove-user alice

# grant admin privileges
renews admin add-admin alice

# revoke admin privileges
renews admin remove-admin alice

# add moderator permissions
renews admin add-moderator alice 'rust.*'

# remove moderator permissions
renews admin remove-moderator alice 'rust.*'
```

Use `--init` to create the article, authentication and peer state databases
without starting the server:

```bash
renews --init --config /opt/renews/config.toml
```
