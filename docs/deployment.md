# Deployment Guide

This document covers deployment strategies and operational considerations for Renews NNTP server.

## System Requirements

### Minimum Requirements
- Linux/Unix system with systemd (recommended)
- 1 GB RAM
- 10 GB disk space (depending on retention policies)
- Rust toolchain for building from source

### Recommended Requirements
- 2+ GB RAM for high-traffic servers
- SSD storage for better performance
- Dedicated database server for PostgreSQL backend
- TLS certificates for secure connections

## Installation

### From Source

1. **Install dependencies:**
   ```bash
   # Ubuntu/Debian
   sudo apt-get install build-essential pkg-config libssl-dev libsqlite3-dev
   
   # CentOS/RHEL
   sudo yum install gcc openssl-devel sqlite-devel
   ```

2. **Install Rust:**
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source ~/.cargo/env
   ```

3. **Build Renews:**
   ```bash
   git clone https://github.com/Chemiseblanc/renews.git
   cd renews
   cargo build --release --features websocket,postgres
   ```

4. **Install binary:**
   ```bash
   sudo cp target/release/renews /usr/local/bin/
   sudo chmod +x /usr/local/bin/renews
   ```

## Configuration Setup

### Create Directory Structure

```bash
sudo mkdir -p /opt/renews/data
sudo mkdir -p /opt/renews/logs
sudo mkdir -p /etc/renews
```

### Create User Account

```bash
sudo useradd --system --home /opt/renews --shell /bin/false renews
sudo chown -R renews:renews /opt/renews
```

### Basic Configuration

Create `/etc/renews/config.toml`:

```toml
addr = ":119"
site_name = "news.example.com"
db_path = "sqlite:///opt/renews/data/news.db"
auth_db_path = "sqlite:///opt/renews/data/auth.db"
peer_db_path = "sqlite:///opt/renews/data/peers.db"
idle_timeout_secs = 600
default_retention_days = 30
default_max_article_bytes = "1M"
```

### Initialize Databases

```bash
sudo -u renews renews --config /etc/renews/config.toml --init
```

## Systemd Service

### Service File

Create `/etc/systemd/system/renews.service`:

```ini
[Unit]
Description=Renews NNTP Server
Documentation=https://github.com/Chemiseblanc/renews
After=network.target
Wants=network.target

[Service]
Type=simple
User=renews
Group=renews
ExecStart=/usr/local/bin/renews --config /etc/renews/config.toml
ExecReload=/bin/kill -HUP $MAINPID
WorkingDirectory=/opt/renews
Restart=on-failure
RestartSec=5

# Security settings
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/renews
CapabilityBoundingSet=CAP_NET_BIND_SERVICE

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=renews

[Install]
WantedBy=multi-user.target
```

### Enable and Start Service

```bash
sudo systemctl daemon-reload
sudo systemctl enable renews
sudo systemctl start renews
sudo systemctl status renews
```

## Systemd Socket Activation

Systemd socket activation allows Renews to listen on privileged ports (like 119 and 563) without running as root. The systemd daemon listens on the ports and passes connections to Renews when they arrive.

### Benefits

- **Security**: Run Renews as a non-root user while listening on privileged ports
- **Flexibility**: Start/stop the service without losing active connections
- **Resource Efficiency**: Sockets are created only when needed

### Socket Configuration

Create socket unit files for NNTP and NNTPS:

**`/etc/systemd/system/renews-nntp.socket`:**
```ini
[Unit]
Description=Renews NNTP Server Socket
PartOf=renews.service

[Socket]
ListenStream=119
BindIPv6Only=both
FDName=nntp

[Install]
WantedBy=sockets.target
```

**`/etc/systemd/system/renews-nntps.socket`:**
```ini
[Unit]
Description=Renews NNTPS Server Socket  
PartOf=renews.service

[Socket]
ListenStream=563
BindIPv6Only=both
FDName=nntps

[Install]
WantedBy=sockets.target
```

### Service Configuration for Socket Activation

Update `/etc/systemd/system/renews.service`:

```ini
[Unit]
Description=Renews NNTP Server
Documentation=https://github.com/Chemiseblanc/renews
After=network.target
Wants=renews-nntp.socket
Wants=renews-nntps.socket

[Service]
Type=simple
User=renews
Group=renews
ExecStart=/usr/local/bin/renews --config /etc/renews/config.toml
ExecReload=/bin/kill -HUP $MAINPID
WorkingDirectory=/opt/renews
Restart=on-failure
RestartSec=5
RestartPreventExitStatus=1

# Security settings
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/renews

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=renews

[Install]
WantedBy=multi-user.target
```

### Application Configuration

Update `/etc/renews/config.toml` to use systemd sockets:

```toml
# Use systemd socket activation
addr = "systemd://renews-nntp.socket"
tls_addr = "systemd://renews-nntps.socket"

# Traditional direct binding (comment out when using socket activation)
# addr = ":119"
# tls_addr = ":563"

site_name = "news.example.com"
tls_cert = "/etc/renews/server.crt"
tls_key = "/etc/renews/server.key"
```

### Activate Socket Services

```bash
# Enable and start socket units
sudo systemctl daemon-reload
sudo systemctl enable renews-nntp.socket renews-nntps.socket
sudo systemctl start renews-nntp.socket renews-nntps.socket

# Enable the main service (will be started on demand)
sudo systemctl enable renews.service

# Check socket status
sudo systemctl status renews-nntp.socket renews-nntps.socket
```

### Verify Socket Activation

Test that sockets are listening:
```bash
# Check listening sockets
sudo ss -tlnp | grep -E ':(119|563)'

# Test connection triggers service start
telnet localhost 119
# Service should start automatically

# Check service status
sudo systemctl status renews.service
```

### Migration from Direct Binding

If migrating from direct binding to socket activation:

1. **Stop the existing service:**
   ```bash
   sudo systemctl stop renews
   ```

2. **Update configuration** to use `systemd://` addresses

3. **Install socket unit files** as shown above

4. **Start socket services:**
   ```bash
   sudo systemctl start renews-nntp.socket renews-nntps.socket
   ```

5. **Test the setup** - the service will start automatically on first connection

### Troubleshooting Socket Activation

Common issues and solutions:

1. **Socket not found error:**
   ```bash
   # Check socket unit status
   sudo systemctl status renews-nntp.socket
   
   # Ensure socket is active
   sudo systemctl start renews-nntp.socket
   ```

2. **Permission denied on socket:**
   ```bash
   # Check that the service runs as the correct user
   sudo systemctl edit renews.service
   # Ensure User=renews is set
   ```

3. **Service fails to start with socket:**
   ```bash
   # Check service logs
   sudo journalctl -u renews.service -f
   
   # Verify socket configuration in Renews config
   # Ensure systemd:// URLs match socket unit names
   ```

4. **Test socket activation manually:**
   ```bash
   # Check what systemd passes to the service
   sudo systemd-socket-activate -l 119 /usr/local/bin/renews --config /etc/renews/config.toml
   ```

## TLS Configuration

### Generate Self-Signed Certificate (Testing)

```bash
sudo openssl req -x509 -nodes -days 365 -newkey rsa:2048 \
  -keyout /etc/renews/server.key \
  -out /etc/renews/server.crt \
  -subj "/CN=news.example.com"

sudo chown renews:renews /etc/renews/server.*
sudo chmod 600 /etc/renews/server.key
```

### Let's Encrypt Certificate (Production)

```bash
# Install certbot
sudo apt-get install certbot

# Obtain certificate
sudo certbot certonly --standalone -d news.example.com

# Create symbolic links
sudo ln -s /etc/letsencrypt/live/news.example.com/fullchain.pem /etc/renews/server.crt
sudo ln -s /etc/letsencrypt/live/news.example.com/privkey.pem /etc/renews/server.key
```

### Update Configuration

Add to `/etc/renews/config.toml`:

```toml
tls_addr = ":563"
tls_cert = "/etc/renews/server.crt"
tls_key = "/etc/renews/server.key"
```

Reload configuration:
```bash
sudo systemctl reload renews
```

## Database Setup

### SQLite (Default)
SQLite databases are created automatically. Ensure proper file permissions:

```bash
sudo chown renews:renews /opt/renews/data/*.db
sudo chmod 644 /opt/renews/data/*.db
```

### PostgreSQL Backend

1. **Install PostgreSQL:**
   ```bash
   sudo apt-get install postgresql postgresql-contrib
   ```

2. **Create database and user:**
   ```sql
   sudo -u postgres psql
   CREATE DATABASE renews;
   CREATE USER renews WITH PASSWORD 'secure_password';
   GRANT ALL PRIVILEGES ON DATABASE renews TO renews;
   \q
   ```

3. **Update configuration:**
   ```toml
   db_path = "postgres://renews:secure_password@localhost/renews"
   auth_db_path = "postgres://renews:secure_password@localhost/renews"
   ```

## Firewall Configuration

### UFW (Ubuntu)
```bash
sudo ufw allow 119/tcp    # NNTP
sudo ufw allow 563/tcp    # NNTPS
sudo ufw allow 8080/tcp   # WebSocket (if enabled)
```

### firewalld (CentOS/RHEL)
```bash
sudo firewall-cmd --permanent --add-port=119/tcp
sudo firewall-cmd --permanent --add-port=563/tcp
sudo firewall-cmd --permanent --add-port=8080/tcp
sudo firewall-cmd --reload
```

### iptables
```bash
sudo iptables -A INPUT -p tcp --dport 119 -j ACCEPT
sudo iptables -A INPUT -p tcp --dport 563 -j ACCEPT
sudo iptables -A INPUT -p tcp --dport 8080 -j ACCEPT
```

## Monitoring and Maintenance

### Log Monitoring

View logs:
```bash
sudo journalctl -u renews -f
```

Log rotation is handled automatically by systemd.

### Database Maintenance

#### SQLite Optimization
```bash
sudo -u renews sqlite3 /opt/renews/data/news.db "VACUUM;"
sudo -u renews sqlite3 /opt/renews/data/news.db "ANALYZE;"
```

#### PostgreSQL Maintenance
```sql
-- Connect to database
sudo -u postgres psql renews

-- Update statistics
ANALYZE;

-- Reclaim space
VACUUM;
```

### Backup Strategy

#### SQLite Backup
```bash
#!/bin/bash
BACKUP_DIR="/opt/renews/backups"
DATE=$(date +%Y%m%d_%H%M%S)

mkdir -p $BACKUP_DIR
sudo -u renews sqlite3 /opt/renews/data/news.db ".backup $BACKUP_DIR/news_$DATE.db"
sudo -u renews sqlite3 /opt/renews/data/auth.db ".backup $BACKUP_DIR/auth_$DATE.db"

# Compress and remove old backups
gzip $BACKUP_DIR/*_$DATE.db
find $BACKUP_DIR -name "*.gz" -mtime +30 -delete
```

#### PostgreSQL Backup
```bash
#!/bin/bash
BACKUP_DIR="/opt/renews/backups"
DATE=$(date +%Y%m%d_%H%M%S)

mkdir -p $BACKUP_DIR
sudo -u postgres pg_dump renews | gzip > $BACKUP_DIR/renews_$DATE.sql.gz

# Remove old backups
find $BACKUP_DIR -name "*.sql.gz" -mtime +30 -delete
```

### Performance Monitoring

Monitor server performance:
```bash
# Connection count
netstat -an | grep :119 | wc -l

# Database size
du -sh /opt/renews/data/

# Memory usage
ps aux | grep renews

# Disk I/O
iotop -p $(pidof renews)
```

## Scaling Considerations

### High Availability Setup

1. **Load Balancer** - Use HAProxy or nginx for connection distribution
2. **Shared Storage** - PostgreSQL with replication
3. **Peer Synchronization** - Configure multiple servers as peers

### Performance Tuning

#### Configuration Optimizations
```toml
# Increase connection timeout for busy servers
idle_timeout_secs = 1200

# Optimize retention for high-volume groups
[[group_settings]]
pattern = "alt.binaries.*"
retention_days = 3
max_article_bytes = "50M"
```

#### System Optimizations
```bash
# Increase file descriptor limits
echo "renews soft nofile 65536" >> /etc/security/limits.conf
echo "renews hard nofile 65536" >> /etc/security/limits.conf

# Optimize TCP settings for high connection counts
echo "net.core.somaxconn = 1024" >> /etc/sysctl.conf
echo "net.ipv4.tcp_max_syn_backlog = 1024" >> /etc/sysctl.conf
sysctl -p
```

## Troubleshooting

### Common Issues

1. **Permission Denied on Database**
   ```bash
   sudo chown -R renews:renews /opt/renews/data/
   ```

2. **Port Already in Use**
   ```bash
   sudo netstat -tulpn | grep :119
   sudo systemctl stop existing-service
   ```

3. **TLS Certificate Issues**
   ```bash
   # Check certificate validity
   openssl x509 -in /etc/renews/server.crt -text -noout
   
   # Verify private key matches
   openssl rsa -in /etc/renews/server.key -check
   ```

4. **Database Connection Failed**
   ```bash
   # Test database connectivity
   sudo -u renews renews --config /etc/renews/config.toml --check
   ```

### Debug Mode

Enable verbose logging by setting environment variable:
```bash
export RUST_LOG=debug
sudo -E systemctl restart renews
```