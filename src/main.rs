use std::error::Error;
use std::sync::Arc;

use tokio::net::TcpListener;

use renews::storage::sqlite::SqliteStorage;
use renews::storage::Storage;
use renews::config::Config;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let cfg = Config::from_file("config.toml")?;
    let storage = Arc::new(SqliteStorage::new("sqlite:news.db").await?);
    for g in &cfg.groups {
        storage.add_group(g).await?;
    }
    let addr = format!("127.0.0.1:{}", cfg.port);
    let listener = TcpListener::bind(&addr).await?;
    loop {
        let (socket, _) = listener.accept().await?;
        let storage = storage.clone();
        tokio::spawn(async move {
            if let Err(e) = renews::handle_client(socket, storage).await {
                eprintln!("client error: {e}");
            }
        });
    }
}


