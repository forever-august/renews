use renews::config::Config;

#[test]
fn retention_rules_match() {
    let toml = r#"port = 1199
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
