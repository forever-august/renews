#[cfg(feature = "websocket")]
mod websocket_bridge {
    use crate::utils;
    use futures_util::{SinkExt, StreamExt};
    use renews::{config::Config, ws};
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tokio_tungstenite::{connect_async, tungstenite::Message};

    fn free_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    #[tokio::test]
    async fn quit_via_websocket() {
        let (storage, auth) = utils::setup().await;
        let (nntp_addr, _, nntp_handle) = utils::start_server(
            storage,
            auth,
            toml::from_str("addr=\":119\"").unwrap(),
            false,
        )
        .await;
        let ws_port = free_port();
        let cfg: Config = toml::from_str(&format!(
            "addr=\"127.0.0.1:{}\"\nws_addr=\":{}\"",
            nntp_addr.port(),
            ws_port
        ))
        .unwrap();
        let cfg = Arc::new(RwLock::new(cfg));
        let ws_handle = tokio::spawn(ws::run_ws_bridge(cfg));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let url = format!("ws://127.0.0.1:{ws_port}");
        let (mut stream, _) = connect_async(url).await.unwrap();
        let msg = stream.next().await.unwrap().unwrap();
        assert_eq!(
            msg,
            Message::Binary(b"201 NNTP Service Ready - no posting allowed\r\n".to_vec())
        );
        stream.send(Message::Text("QUIT\r\n".into())).await.unwrap();
        let msg = stream.next().await.unwrap().unwrap();
        assert_eq!(msg, Message::Binary(b"205 closing connection\r\n".to_vec()));
        ws_handle.abort();
        nntp_handle.await.unwrap();
    }
}
