use std::error::Error;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use renews::{parse_command, parse_message, Message};
use renews::storage::{self, sqlite::SqliteStorage, DynStorage};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let storage = Arc::new(SqliteStorage::new("sqlite:news.db").await?);
    let listener = TcpListener::bind("127.0.0.1:1199").await?;
    loop {
        let (socket, _) = listener.accept().await?;
        let storage = storage.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_client(socket, storage).await {
                eprintln!("client error: {e}");
            }
        });
    }
}

async fn handle_client(
    mut socket: TcpStream,
    storage: DynStorage,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let (read_half, mut write_half) = socket.split();
    let mut reader = BufReader::new(read_half);
    write_half.write_all(b"200 NNTP Service Ready\r\n").await?;
    let mut line = String::new();
    let mut current_group = String::from("misc");
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        let (_, cmd) = match parse_command(trimmed) {
            Ok(c) => c,
            Err(_) => {
                write_half.write_all(b"500 syntax error\r\n").await?;
                continue;
            }
        };
        match cmd.name.as_str() {
            "QUIT" => {
                write_half.write_all(b"205 closing connection\r\n").await?;
                break;
            }
            "GROUP" => {
                if let Some(name) = cmd.args.get(0) {
                    current_group = name.clone();
                    write_half
                        .write_all(format!("211 0 1 1 {}\r\n", name).as_bytes())
                        .await?;
                } else {
                    write_half.write_all(b"411 missing group\r\n").await?;
                }
            }
            "ARTICLE" => {
                if let Some(arg) = cmd.args.get(0) {
                    if arg.starts_with('<') && arg.ends_with('>') {
                        if let Some(article) = storage.get_article_by_id(arg).await? {
                            write_half
                                .write_all(b"220 0 article follows\r\n")
                                .await?;
                            for (k, v) in article.headers.iter() {
                                write_half
                                    .write_all(format!("{}: {}\r\n", k, v).as_bytes())
                                    .await?;
                            }
                            write_half.write_all(b"\r\n").await?;
                            write_half.write_all(article.body.as_bytes()).await?;
                            write_half.write_all(b"\r\n.\r\n").await?;
                        } else {
                            write_half.write_all(b"430 no such article\r\n").await?;
                        }
                    } else if let Ok(num) = arg.parse::<u64>() {
                        if let Some(article) =
                            storage.get_article_by_number(&current_group, num).await?
                        {
                            write_half
                                .write_all(b"220 0 article follows\r\n")
                                .await?;
                            for (k, v) in article.headers.iter() {
                                write_half
                                    .write_all(format!("{}: {}\r\n", k, v).as_bytes())
                                    .await?;
                            }
                            write_half.write_all(b"\r\n").await?;
                            write_half.write_all(article.body.as_bytes()).await?;
                            write_half.write_all(b"\r\n.\r\n").await?;
                        } else {
                            write_half.write_all(b"423 no such article number in this group\r\n").await?;
                        }
                    } else {
                        write_half.write_all(b"501 invalid id\r\n").await?;
                    }
                } else {
                    write_half.write_all(b"501 missing id\r\n").await?;
                }
            }
            "POST" => {
                write_half
                    .write_all(b"340 send article to be posted. End with <CR-LF>.<CR-LF>\r\n")
                    .await?;
                let mut msg = String::new();
                loop {
                    line.clear();
                    reader.read_line(&mut line).await?;
                    if line.trim_end() == "." {
                        break;
                    }
                    msg.push_str(&line);
                }
                let (_, message) = parse_message(&msg).map_err(|_| "invalid message")?;
                let _ = storage.store_article(&current_group, &message).await?;
                write_half.write_all(b"240 article received\r\n").await?;
            }
            _ => {
                write_half.write_all(b"500 command not recognized\r\n").await?;
            }
        }
    }
    Ok(())
}

