use std::error::Error;
use std::sync::Arc;

use tokio::net::TcpListener;

use renews::storage::sqlite::SqliteStorage;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let storage = Arc::new(SqliteStorage::new("sqlite:news.db").await?);
    let listener = TcpListener::bind("127.0.0.1:1199").await?;
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


