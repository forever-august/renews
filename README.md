# Renews

Renews is a minimal NNTP server implemented in Rust.  It stores articles in an
SQLite database and supports a configurable set of newsgroups.  The server can
optionally accept NNTP over TLS when the TLS parameters are provided.

## Building

```bash
cargo build --release
```
This produces the `renews` binary in `target/release/`.

## Configuration

Configuration is loaded from the file specified with `--config` (defaults to
`/etc/renews.toml`). The
following keys are recognised:

- `port` - TCP port for plain NNTP connections.
- `groups` - list of newsgroups that will be created on start-up.
- `db_path` - path to the SQLite database file. Defaults to `/var/spool/renews.db`.
- `tls_port` - optional port for NNTP over TLS.
- `tls_cert` - path to the TLS certificate in PEM format.
- `tls_key` - path to the TLS private key in PEM format.
- `default_retention_days` - default number of days to keep articles.
- `default_max_article_bytes` - default maximum article size in bytes. A `K`,
  `M` or `G` suffix may be used to specify kilobytes, megabytes or gigabytes.
- `group_settings` - list of per-group rules which can match a `group` exactly or a
  `pattern` using wildmat syntax to override retention and size defaults.

An example configuration is provided in the repository:

```toml
port = 1199
groups = ["misc.news"]
db_path = "/var/spool/renews.db"
tls_port = 563
tls_cert = "cert.pem"
tls_key = "key.pem"
default_retention_days = 30
default_max_article_bytes = "1M"

[[group_settings]]
pattern = "short.*"
retention_days = 7

[[group_settings]]
group = "misc.news"
retention_days = 60
max_article_bytes = "2M"
```

`tls_port`, `tls_cert` and `tls_key` must all be set for TLS support to be
enabled.

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
WorkingDirectory=/opt/renews
Restart=on-failure
User=renews
Group=renews

[Install]
WantedBy=multi-user.target
```

Install the file as `/etc/systemd/system/renews.service` and run
`systemctl enable --now renews` to start the server at boot.

