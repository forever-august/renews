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

Configuration is loaded from `config.toml` in the working directory.  The
following keys are recognised:

- `port` - TCP port for plain NNTP connections.
- `groups` - list of newsgroups that will be created on start-up.
- `db_path` - path to the SQLite database file. Defaults to `/var/spool/renews.db`.
- `tls_port` - optional port for NNTP over TLS.
- `tls_cert` - path to the TLS certificate in PEM format.
- `tls_key` - path to the TLS private key in PEM format.

An example configuration is provided in the repository:

```toml
port = 1199
groups = ["misc.news"]
db_path = "/var/spool/renews.db"
tls_port = 563
tls_cert = "cert.pem"
tls_key = "key.pem"
```

`tls_port`, `tls_cert` and `tls_key` must all be set for TLS support to be
enabled.

## Deployment with systemd

The service expects `config.toml` in its working directory.  A simple systemd
unit may look like this:

```ini
[Unit]
Description=Renews NNTP server
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/renews
WorkingDirectory=/opt/renews
Restart=on-failure
User=renews
Group=renews

[Install]
WantedBy=multi-user.target
```

Install the file as `/etc/systemd/system/renews.service` and run
`systemctl enable --now renews` to start the server at boot.

