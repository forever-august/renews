use renews::config::Config;

#[test]
fn retention_rules_match() {
    let toml = r#"addr = ":119"
default_retention_days = 10
default_max_article_bytes = "1K"
[[group_settings]]
pattern = "foo.*"
retention_days = 1
[[group_settings]]
group = "foo.bar"
retention_days = 5
max_article_bytes = "20K"
"#;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert_eq!(cfg.retention_for_group("misc").unwrap().num_days(), 10);
    assert_eq!(cfg.retention_for_group("foo.test").unwrap().num_days(), 1);
    assert_eq!(cfg.retention_for_group("foo.bar").unwrap().num_days(), 5);
    assert_eq!(cfg.max_size_for_group("misc"), Some(1024));
    assert_eq!(cfg.max_size_for_group("foo.test"), Some(1024));
    assert_eq!(cfg.max_size_for_group("foo.bar"), Some(20480));
}

#[test]
fn runtime_update_preserves_immutable_fields() {
    let initial = r#"addr = ":119"
db_path = "/tmp/db1"
auth_db_path = "sqlite:///tmp/auth1"
peer_db_path = "/tmp/peer1"

idle_timeout_secs = 600
tls_addr = ":563"
tls_cert = "old.pem"
tls_key = "old.key"
default_retention_days = 10
default_max_article_bytes = 100
[[group_settings]]
group = "misc.news"
retention_days = 5
"#;
    let mut cfg: Config = toml::from_str(initial).unwrap();

    let updated = r#"addr = ":42"
db_path = "/tmp/db2"
auth_db_path = "sqlite:///tmp/auth2"
peer_db_path = "/tmp/peer2"

idle_timeout_secs = 1200
tls_addr = ":9999"
tls_cert = "new.pem"
tls_key = "new.key"
default_retention_days = 1
default_max_article_bytes = 200
[[group_settings]]
group = "misc.news"
retention_days = 1
"#;
    let new_cfg: Config = toml::from_str(updated).unwrap();
    cfg.update_runtime(new_cfg);

    assert_eq!(cfg.addr, ":119");
    assert_eq!(cfg.db_path, "/tmp/db1");
    assert_eq!(cfg.auth_db_path, "sqlite:///tmp/auth1");
    assert_eq!(cfg.peer_db_path, "/tmp/peer1");
    assert_eq!(cfg.idle_timeout_secs, 1200);
    assert_eq!(cfg.tls_addr.as_deref(), Some(":563"));
    assert_eq!(cfg.tls_cert.as_deref(), Some("new.pem"));
    assert_eq!(cfg.tls_key.as_deref(), Some("new.key"));
    assert_eq!(cfg.default_retention_days, Some(1));
    assert_eq!(cfg.default_max_article_bytes, Some(200));
    assert_eq!(cfg.group_settings[0].retention_days, Some(1));
}

#[test]
fn default_paths() {
    let cfg: Config = toml::from_str("addr=\":119\"").unwrap();
    assert_eq!(cfg.db_path, "sqlite:///var/renews/news.db");
    assert_eq!(cfg.auth_db_path, "sqlite:///var/renews/auth.db");
    assert_eq!(cfg.peer_db_path, "sqlite:///var/renews/peers.db");
    assert_eq!(cfg.idle_timeout_secs, 600);
}

#[test]
fn idle_timeout_configuration() {
    let toml = r#"addr = ":119"
idle_timeout_secs = 300
"#;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert_eq!(cfg.idle_timeout_secs, 300);
}

#[test]
fn idle_timeout_runtime_update() {
    let initial = r#"addr = ":119"
idle_timeout_secs = 600
"#;
    let mut cfg: Config = toml::from_str(initial).unwrap();

    let updated = r#"addr = ":42"
idle_timeout_secs = 1200
"#;
    let new_cfg: Config = toml::from_str(updated).unwrap();
    cfg.update_runtime(new_cfg);

    // Addr should be preserved (immutable)
    assert_eq!(cfg.addr, ":119");
    // Idle timeout should be updated (runtime-adjustable)
    assert_eq!(cfg.idle_timeout_secs, 1200);
}

#[test]
fn peer_connection_string_allows_credentials() {
    let toml = r#"addr = ":119"
[[peers]]
sitename = "u:p@news.example.com:563"
"#;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert_eq!(cfg.peers.len(), 1);
    assert_eq!(cfg.peers[0].sitename, "u:p@news.example.com:563");
}

#[test]
fn env_substitution() {
    use std::fs::write;
    use tempfile::tempdir;

    unsafe { std::env::set_var("TEST_ADDR", ":4242") };
    let dir = tempdir().unwrap();
    let cfg_path = dir.path().join("cfg.toml");
    write(&cfg_path, "addr = \"$ENV{TEST_ADDR}\"").unwrap();
    let cfg = Config::from_file(cfg_path.to_str().unwrap()).unwrap();
    assert_eq!(cfg.addr, ":4242");
}

#[test]
fn file_substitution() {
    use std::fs::{File, write};
    use std::io::Write as _;
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let val_path = dir.path().join("val");
    write(&val_path, ":5050").unwrap();
    let cfg_path = dir.path().join("cfg.toml");
    let mut f = File::create(&cfg_path).unwrap();
    write!(f, "addr = \"$FILE{{{}}}\"", val_path.display()).unwrap();
    let cfg = Config::from_file(cfg_path.to_str().unwrap()).unwrap();
    assert_eq!(cfg.addr, ":5050");
}

#[test]
fn peer_cron_schedule_configuration() {
    let cfg_str = r#"addr = ":119"
peer_sync_schedule = "0 0 * * * *"
[[peers]]
sitename = "peer1.example.com"
patterns = ["*"]
sync_schedule = "0 */30 * * * *"

[[peers]]
sitename = "peer2.example.com"
patterns = ["misc.*"]
# Uses default schedule
"#;
    let cfg: Config = toml::from_str(cfg_str).unwrap();
    assert_eq!(cfg.peer_sync_schedule, "0 0 * * * *");
    assert_eq!(cfg.peers.len(), 2);
    assert_eq!(cfg.peers[0].sitename, "peer1.example.com");
    assert_eq!(
        cfg.peers[0].sync_schedule,
        Some("0 */30 * * * *".to_string())
    );
    assert_eq!(cfg.peers[1].sitename, "peer2.example.com");
    assert_eq!(cfg.peers[1].sync_schedule, None);
}
