#[path = "integration/auth.rs"]
mod auth;
#[path = "integration/cancel_lock.rs"]
mod cancel_lock;
#[path = "integration/control.rs"]
mod control;
#[path = "integration/handler_failures.rs"]
mod handler_failures;
#[path = "integration/idle_timeout.rs"]
mod idle_timeout;
#[path = "integration/max_size.rs"]
mod max_size;
#[path = "integration/moderated.rs"]
mod moderated;
#[path = "integration/peers.rs"]
mod peers;
#[path = "integration/resource_exhaustion.rs"]
mod resource_exhaustion;
#[path = "integration/retention.rs"]
mod retention;
#[path = "integration/storage.rs"]
mod storage;
#[path = "integration/tls.rs"]
mod tls;
#[path = "utils.rs"]
mod utils;
#[cfg(feature = "websocket")]
#[path = "integration/ws.rs"]
mod ws;
