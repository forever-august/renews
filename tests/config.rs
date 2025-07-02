use renews::config::Config;

#[test]
fn retention_rules_match() {
    let toml = r#"port = 1199
default_retention_days = 10
[[retention]]
pattern = "foo.*"
days = 1
[[retention]]
group = "foo.bar"
days = 5
"#;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert_eq!(cfg.retention_for_group("misc").unwrap().num_days(), 10);
    assert_eq!(cfg.retention_for_group("foo.test").unwrap().num_days(), 1);
    assert_eq!(cfg.retention_for_group("foo.bar").unwrap().num_days(), 5);
}
