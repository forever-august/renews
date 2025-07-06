use std::error::Error;

use clap::{Parser, Subcommand};

use renews::auth::sqlite::SqliteAuth;
use renews::auth::AuthProvider;
use renews::config::Config;
use renews::storage::sqlite::SqliteStorage;
use renews::storage::Storage;
use renews::server;

#[derive(Parser)]
struct Args {
    /// Path to the configuration file
    #[arg(long, env = "RENEWS_CONFIG", default_value = "/etc/renews.toml")]
    config: String,
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
    let db_conn = format!("sqlite:{}", cfg.db_path);
    let storage = SqliteStorage::new(&db_conn).await?;
    let auth_path = &cfg.auth_db_path;
    let auth_conn = format!("sqlite:{auth_path}");
    let auth = SqliteAuth::new(&auth_conn).await?;
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

#[allow(clippy::too_many_lines)]
#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    let cfg_path = args.config.clone();
    let cfg_initial = Config::from_file(&cfg_path)?;

    if let Some(Command::Admin(cmd)) = args.command {
        run_admin(cmd, &cfg_initial).await?;
        return Ok(());
    }

    server::run(cfg_initial, cfg_path).await
}
