use std::error::Error;

use clap::{Parser, Subcommand};

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
    /// Allow posting without TLS for development
    #[arg(long)]
    allow_posting_insecure_connections: bool,
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
    /// Add a newsgroup
    AddGroup {
        group: String,
        #[arg(long)]
        moderated: bool,
    },
    /// Remove a newsgroup
    RemoveGroup { group: String },
    /// Add a user
    AddUser { username: String, password: String },
    /// Remove a user
    RemoveUser { username: String },
    /// Grant admin privileges to a user with the provided PGP public key
    AddAdmin { username: String, key: String },
    /// Revoke admin privileges from a user
    RemoveAdmin { username: String },
    /// Add a moderator pattern for a user
    AddModerator { username: String, pattern: String },
    /// Remove a moderator pattern for a user
    RemoveModerator { username: String, pattern: String },
}

async fn run_admin(cmd: AdminCommand, cfg: &Config) -> Result<(), Box<dyn Error + Send + Sync>> {
    let storage = storage::open(&cfg.db_path).await?;
    let auth = auth::open(&cfg.auth_db_path).await?;
    match cmd {
        AdminCommand::AddGroup { group, moderated } => storage.add_group(&group, moderated).await?,
        AdminCommand::RemoveGroup { group } => storage.remove_group(&group).await?,
        AdminCommand::AddUser { username, password } => auth.add_user(&username, &password).await?,
        AdminCommand::RemoveUser { username } => auth.remove_user(&username).await?,
        AdminCommand::AddAdmin { username, key } => auth.add_admin(&username, &key).await?,
        AdminCommand::RemoveAdmin { username } => auth.remove_admin(&username).await?,
        AdminCommand::AddModerator { username, pattern } => {
            auth.add_moderator(&username, &pattern).await?;
        }
        AdminCommand::RemoveModerator { username, pattern } => {
            auth.remove_moderator(&username, &pattern).await?;
        }
    }
    Ok(())
}

async fn run_init(cfg: &Config) -> Result<(), Box<dyn Error + Send + Sync>> {
    storage::open(&cfg.db_path).await?;
    auth::open(&cfg.auth_db_path).await?;
    let peer_db = renews::peers::PeerDb::new(&cfg.peer_db_path).await?;
    let names: Vec<String> = cfg.peers.iter().map(|p| p.sitename.clone()).collect();
    peer_db.sync_config(&names).await?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    let cfg_path = args.config.clone();
    let mut cfg_initial = Config::from_file(&cfg_path)?;
    
    // Override config with CLI flag if provided
    if args.allow_posting_insecure_connections {
        cfg_initial.allow_posting_insecure_connections = true;
    }

    if args.init {
        run_init(&cfg_initial).await?;
        return Ok(());
    }

    if let Some(cmd) = args.command {
        match cmd {
            Command::Admin(c) => {
                run_admin(c, &cfg_initial).await?;
                return Ok(());
            }
        }
    }

    server::run(cfg_initial, cfg_path).await
}
