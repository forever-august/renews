use renews::config::Config;

#[test]
fn retention_rules_match() {
    let toml = r#"port = 119
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
    let initial = r#"port = 119
db_path = "/tmp/db1"
auth_db_path = "/tmp/auth1"
peer_db_path = "/tmp/peer1"
peer_sync_secs = 1800
tls_port = 563
tls_cert = "old.pem"
tls_key = "old.key"
default_retention_days = 10
default_max_article_bytes = 100
[[group_settings]]
group = "misc.news"
retention_days = 5
"#;
    let mut cfg: Config = toml::from_str(initial).unwrap();

    let updated = r#"port = 42
db_path = "/tmp/db2"
auth_db_path = "/tmp/auth2"
peer_db_path = "/tmp/peer2"
peer_sync_secs = 3600
tls_port = 9999
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

    assert_eq!(cfg.port, 119);
    assert_eq!(cfg.db_path, "/tmp/db1");
    assert_eq!(cfg.auth_db_path, "/tmp/auth1");
    assert_eq!(cfg.peer_db_path, "/tmp/peer1");
    assert_eq!(cfg.peer_sync_secs, 3600);
    assert_eq!(cfg.tls_port, Some(563));
    assert_eq!(cfg.tls_cert.as_deref(), Some("new.pem"));
    assert_eq!(cfg.tls_key.as_deref(), Some("new.key"));
    assert_eq!(cfg.default_retention_days, Some(1));
    assert_eq!(cfg.default_max_article_bytes, Some(200));
    assert_eq!(cfg.group_settings[0].retention_days, Some(1));
}

#[test]
fn default_paths() {
    let cfg: Config = toml::from_str("port=119").unwrap();
    assert_eq!(cfg.db_path, "sqlite:///var/renews/news.db");
    assert_eq!(cfg.auth_db_path, "sqlite:///var/renews/auth.db");
    assert_eq!(cfg.peer_db_path, "sqlite:///var/renews/peers.db");
    assert_eq!(cfg.peer_sync_secs, 3600);
}

#[test]
fn peer_auth_fields() {
    let toml = r#"port = 119
[[peers]]
sitename = "news.example.com"
username = "u"
password = "p"
"#;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert_eq!(cfg.peers.len(), 1);
    assert_eq!(cfg.peers[0].username.as_deref(), Some("u"));
    assert_eq!(cfg.peers[0].password.as_deref(), Some("p"));
}
