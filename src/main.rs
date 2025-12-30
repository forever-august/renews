use anyhow::Result;

use clap::{Parser, Subcommand};
use tokio::runtime::Runtime;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use renews::auth;
use renews::config::{Config, DEFAULT_LOG_FILTER, parse_duration_secs, parse_size};
use renews::limits::UserLimits;
use renews::server;
use renews::storage;

#[derive(Parser)]
struct Args {
    /// Path to the configuration file
    #[arg(long, env = "RENEWS_CONFIG", default_value = "/etc/renews.toml")]
    config: String,
    /// Initialize databases and exit
    #[arg(long)]
    init: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Administrative actions
    #[command(subcommand)]
    Admin(AdminCommand),
}

#[derive(Subcommand)]
enum AdminCommand {
    /// Add newsgroups
    AddGroup {
        /// First group name
        group: String,
        /// Additional group names
        #[arg(required = false)]
        groups: Vec<String>,
    },
    /// Remove newsgroups matching a wildmat pattern
    RemoveGroup {
        /// Wildmat pattern for groups to remove
        wildmat: String,
    },
    /// Add a user with optional PGP key
    AddUser {
        user: String,
        pass: String,
        /// Optional PGP public key
        #[arg(long)]
        pgp_key: Option<String>,
    },
    /// Update user password
    UpdatePassword { user: String, new_pass: String },
    /// Remove a user
    RemoveUser { user: String },
    /// Update user's PGP key
    UpdateKey { user: String, pgp_key: String },
    /// Set moderation status for a group
    SetModerated { group: String, moderated: String },
    /// Grant admin privileges to a user
    AddAdmin { user: String },
    /// Revoke admin privileges from a user
    RemoveAdmin { user: String },
    /// Add a moderator for a group
    AddModerator { user: String, group: String },
    /// Remove a moderator for a group
    RemoveModerator { user: String, group: String },
    /// Set per-user limits (posting permission, bandwidth, connections)
    SetLimits {
        /// Username to set limits for
        user: String,
        /// Allow/disallow posting (true/false)
        #[arg(long)]
        allow_posting: Option<String>,
        /// Max simultaneous connections (0 = unlimited)
        #[arg(long)]
        max_connections: Option<u32>,
        /// Bandwidth limit (e.g., "10G", "500M", 0 = unlimited)
        #[arg(long)]
        bandwidth_limit: Option<String>,
        /// Bandwidth period (e.g., "30d", "1w", empty = absolute/lifetime)
        #[arg(long)]
        bandwidth_period: Option<String>,
    },
    /// Clear per-user limits (revert to defaults)
    ClearLimits {
        /// Username to clear limits for
        user: String,
    },
    /// Show current usage for a user
    ShowUsage {
        /// Username to show usage for
        user: String,
    },
    /// Reset usage counters for a user
    ResetUsage {
        /// Username to reset usage for
        user: String,
    },
}

async fn run_admin(cmd: AdminCommand, cfg: &Config) -> Result<()> {
    let storage = storage::open(&cfg.db_path).await?;
    let auth = auth::open(&cfg.auth_db_path).await?;
    match cmd {
        AdminCommand::AddGroup { group, groups } => {
            // Add the first group
            storage.add_group(&group, false).await?;
            // Add any additional groups
            for g in groups {
                storage.add_group(&g, false).await?;
            }
        }
        AdminCommand::RemoveGroup { wildmat } => {
            storage.remove_groups_by_pattern(&wildmat).await?;
        }
        AdminCommand::AddUser {
            user,
            pass,
            pgp_key,
        } => {
            auth.add_user_with_key(&user, &pass, pgp_key.as_deref())
                .await?;
        }
        AdminCommand::UpdatePassword { user, new_pass } => {
            auth.update_password(&user, &new_pass).await?;
        }
        AdminCommand::RemoveUser { user } => {
            auth.remove_user(&user).await?;
        }
        AdminCommand::UpdateKey { user, pgp_key } => {
            auth.update_pgp_key(&user, &pgp_key).await?;
        }
        AdminCommand::SetModerated { group, moderated } => {
            let is_moderated = match moderated.to_lowercase().as_str() {
                "true" | "yes" | "1" => true,
                "false" | "no" | "0" => false,
                _ => {
                    return Err(anyhow::anyhow!(
                        "Invalid boolean value: '{moderated}'. Use 'true' or 'false'."
                    ));
                }
            };
            storage.set_group_moderated(&group, is_moderated).await?;
        }
        AdminCommand::AddAdmin { user } => {
            auth.add_admin_without_key(&user).await?;
        }
        AdminCommand::RemoveAdmin { user } => {
            auth.remove_admin(&user).await?;
        }
        AdminCommand::AddModerator { user, group } => {
            auth.add_moderator(&user, &group).await?;
        }
        AdminCommand::RemoveModerator { user, group } => {
            auth.remove_moderator(&user, &group).await?;
        }
        AdminCommand::SetLimits {
            user,
            allow_posting,
            max_connections,
            bandwidth_limit,
            bandwidth_period,
        } => {
            // Get existing limits or create new ones
            let existing = auth.get_user_limits(&user).await?.unwrap_or_default();

            // Parse allow_posting
            let can_post = if let Some(ref val) = allow_posting {
                match val.to_lowercase().as_str() {
                    "true" | "yes" | "1" => true,
                    "false" | "no" | "0" => false,
                    _ => {
                        return Err(anyhow::anyhow!(
                            "Invalid boolean value: '{val}'. Use 'true' or 'false'."
                        ));
                    }
                }
            } else {
                existing.can_post
            };

            // Parse max_connections (0 = unlimited)
            let max_conn = max_connections
                .map(|c| if c == 0 { None } else { Some(c) })
                .unwrap_or(existing.max_connections);

            // Parse bandwidth_limit
            let bw_limit = if let Some(ref val) = bandwidth_limit {
                if val == "0" || val.is_empty() {
                    None
                } else {
                    parse_size(val)
                }
            } else {
                existing.bandwidth_limit
            };

            // Parse bandwidth_period
            let bw_period = if let Some(ref val) = bandwidth_period {
                parse_duration_secs(val)
            } else {
                existing.bandwidth_period_secs
            };

            let limits = UserLimits {
                can_post,
                max_connections: max_conn,
                bandwidth_limit: bw_limit,
                bandwidth_period_secs: bw_period,
            };

            auth.set_user_limits(&user, &limits).await?;
            println!("Limits set for user '{user}':");
            println!("  can_post: {can_post}");
            println!(
                "  max_connections: {}",
                max_conn.map_or("unlimited".to_string(), |c| c.to_string())
            );
            println!(
                "  bandwidth_limit: {}",
                bw_limit.map_or("unlimited".to_string(), format_bytes)
            );
            println!(
                "  bandwidth_period: {}",
                bw_period.map_or("absolute".to_string(), format_duration)
            );
        }
        AdminCommand::ClearLimits { user } => {
            auth.clear_user_limits(&user).await?;
            println!("Limits cleared for user '{user}' (will use defaults)");
        }
        AdminCommand::ShowUsage { user } => {
            let usage = auth.get_user_usage(&user).await?;
            println!("Usage for user '{user}':");
            println!("  uploaded: {}", format_bytes(usage.bytes_uploaded));
            println!("  downloaded: {}", format_bytes(usage.bytes_downloaded));
            println!("  total: {}", format_bytes(usage.total_bandwidth()));
            if let Some(ws) = usage.window_start {
                println!("  window_start: {ws}");
            } else {
                println!("  window_start: (not set)");
            }
        }
        AdminCommand::ResetUsage { user } => {
            auth.reset_user_usage(&user).await?;
            println!("Usage counters reset for user '{user}'");
        }
    }
    Ok(())
}

/// Format bytes into a human-readable string.
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} bytes")
    }
}

/// Format seconds into a human-readable duration string.
fn format_duration(secs: u64) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    const WEEK: u64 = 7 * DAY;

    if secs >= WEEK && secs % WEEK == 0 {
        format!("{} week(s)", secs / WEEK)
    } else if secs >= DAY && secs % DAY == 0 {
        format!("{} day(s)", secs / DAY)
    } else if secs >= HOUR && secs % HOUR == 0 {
        format!("{} hour(s)", secs / HOUR)
    } else if secs >= MINUTE && secs % MINUTE == 0 {
        format!("{} minute(s)", secs / MINUTE)
    } else {
        format!("{secs} second(s)")
    }
}

async fn run_init(cfg: &Config) -> Result<()> {
    storage::open(&cfg.db_path).await?;
    auth::open(&cfg.auth_db_path).await?;
    let peer_db = renews::peers::PeerDb::new(&cfg.peer_db_path).await?;
    let names: Vec<String> = cfg.peers.iter().map(|p| p.sitename.clone()).collect();
    peer_db.sync_config(&names).await?;
    Ok(())
}

/// Initialize the tracing subscriber based on configuration.
///
/// Priority for log level: config file > RUST_LOG env var > default
/// Priority for log format: config file (default: "json")
fn init_tracing(config: &Config) {
    // Determine log level filter
    let filter = config
        .logging
        .level
        .clone()
        .or_else(|| std::env::var("RUST_LOG").ok())
        .unwrap_or_else(|| DEFAULT_LOG_FILTER.to_string());

    let env_filter = EnvFilter::new(&filter);

    // Build subscriber based on configured format
    if config.logging.format == "json" {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    tracing::info!(
        format = %config.logging.format,
        filter = %filter,
        "Logging initialized"
    );
}

#[allow(clippy::too_many_lines)]
fn main() -> Result<()> {
    // Parse args first to get config path
    let args = Args::parse();
    let cfg_path = args.config.clone();

    // Load configuration
    let cfg_initial = match Config::from_file(&cfg_path) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };

    // Initialize tracing based on configuration
    init_tracing(&cfg_initial);

    // Initialize systemd socket support
    if let Err(e) = systemd_socket::init() {
        tracing::warn!(error = %e, "Failed to initialize systemd socket support");
    };

    // Create the runtime based on the configuration
    let runtime_threads = match cfg_initial.get_runtime_threads() {
        Ok(threads) => threads,
        Err(e) => {
            eprintln!("Error determining runtime threads: {e}");
            std::process::exit(1);
        }
    };

    let runtime = if runtime_threads == 1 {
        Runtime::new()?
    } else {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(runtime_threads)
            .enable_all()
            .build()?
    };

    runtime.block_on(async {
        if args.init {
            if let Err(e) = run_init(&cfg_initial).await {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
            return Ok(());
        }

        if let Some(cmd) = args.command {
            match cmd {
                Command::Admin(c) => {
                    if let Err(e) = run_admin(c, &cfg_initial).await {
                        eprintln!("Error: {e}");
                        std::process::exit(1);
                    }
                    return Ok(());
                }
            }
        }

        if let Err(e) = server::run(cfg_initial, cfg_path).await {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }

        Ok(())
    })
}
