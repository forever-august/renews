# Release Packaging

This document describes the packaging system for renews releases.

## Package Types

The renews project provides three types of release packages:

### 1. DEB Package (Debian/Ubuntu)
- **File**: `renews_<version>_amd64.deb`
- **Installation**: `sudo dpkg -i renews_<version>_amd64.deb`
- **Removal**: `sudo dpkg -r renews`

### 2. RPM Package (Red Hat/Fedora/SUSE)
- **File**: `renews-<version>.x86_64.rpm`
- **Installation**: `sudo rpm -i renews-<version>.x86_64.rpm`
- **Removal**: `sudo rpm -e renews`

### 3. Tarball (Generic Linux)
- **File**: `renews-<version>-linux-x86_64.tar.gz`
- **Installation**: Extract and run `sudo ./install.sh`
- **Removal**: Run `sudo ./uninstall.sh`

## File Locations

All packages install files to the following standard locations:

| File | Location |
|------|----------|
| Binary | `/usr/bin/renews` |
| Man page | `/usr/share/man/man1/renews.1` |
| Systemd service | `/lib/systemd/system/renews.service` |
| Config example | `/etc/renews/config.toml.example` |
| Data directory | `/var/lib/renews/` (created during installation) |


## System User

All packages create a system user `renews` with:
- No login shell (`/bin/false`)
- No home directory
- Ownership of `/var/lib/renews/`

## Systemd Integration

The systemd service is installed but not enabled by default. To start renews:

```bash
# Copy and edit the configuration
sudo cp /etc/renews/config.toml.example /etc/renews/config.toml
sudo nano /etc/renews/config.toml

# Initialize the database
sudo -u renews renews --config /etc/renews/config.toml --init

# Start and enable the service
sudo systemctl start renews
sudo systemctl enable renews
```

## Building Packages Locally

To build packages locally, install the required tools:

```bash
# Install packaging tools
cargo install cargo-deb
cargo install cargo-generate-rpm

# Build the project
cargo build --release

# Create DEB package
cargo deb --no-build

# Create RPM package
cargo generate-rpm
```

The generated packages will be located at:
- DEB: `target/debian/renews_<version>_amd64.deb`
- RPM: `target/generate-rpm/renews-<version>.x86_64.rpm`

## Automatic Releases

Release packages are automatically built and uploaded when a new git tag is pushed:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The GitHub Actions workflow will build all three package types and attach them to the GitHub release.

## Configuration

Package metadata is defined in `Cargo.toml`:

- `[package.metadata.deb]` - DEB package configuration
- `[package.metadata.generate-rpm]` - RPM package configuration

The packages are configured to:
- Include proper metadata (description, license, etc.)
- Set correct file permissions
- Handle systemd integration
- Create necessary directories and users
- Provide example configuration files