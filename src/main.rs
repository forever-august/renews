use anyhow::Result;

use clap::{Parser, Subcommand};
use tokio::runtime::Runtime;

use renews::auth;
use renews::config::Config;
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
    }
    Ok(())
}

async fn run_init(cfg: &Config) -> Result<()> {
    storage::open(&cfg.db_path).await?;
    auth::open(&cfg.auth_db_path).await?;
    let peer_db = renews::peers::PeerDb::new(&cfg.peer_db_path).await?;
    let names: Vec<String> = cfg.peers.iter().map(|p| p.sitename.clone()).collect();
    peer_db.sync_config(&names).await?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // Initialize systemd socket support
    if let Err(e) = systemd_socket::init() {
        eprintln!("Warning: Failed to initialize systemd socket support: {e}");
    }

    let args = Args::parse();
    let cfg_path = args.config.clone();

    let cfg_initial = match Config::from_file(&cfg_path) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
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
