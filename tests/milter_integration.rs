//! Integration tests for Milter filter configuration and usage

use renews::config::{Config, FilterConfig};
use renews::filters::factory::create_filter_chain;
use serde_json::json;
use std::io::Write;
use tempfile::NamedTempFile;

#[tokio::test]
async fn test_milter_filter_configuration() {
    // Test creating a filter chain with MilterFilter
    let milter_config = FilterConfig {
        name: "MilterFilter".to_string(),
        parameters: json!({
            "address": "127.0.0.1:8888",
            "use_tls": false,
            "timeout_secs": 30
        }),
    };

    let configs = vec![milter_config];
    let chain = create_filter_chain(&configs).unwrap();
    let names = chain.filter_names();

    assert_eq!(names.len(), 1);
    assert_eq!(names[0], "MilterFilter");
}

#[tokio::test]
async fn test_milter_filter_with_tls_configuration() {
    // Test creating a filter chain with MilterFilter using TLS
    let milter_config = FilterConfig {
        name: "MilterFilter".to_string(),
        parameters: json!({
            "address": "milter.example.com:8888",
            "use_tls": true,
            "timeout_secs": 60
        }),
    };

    let configs = vec![milter_config];
    let chain = create_filter_chain(&configs).unwrap();
    let names = chain.filter_names();

    assert_eq!(names.len(), 1);
    assert_eq!(names[0], "MilterFilter");
}

#[tokio::test]
async fn test_milter_filter_in_pipeline() {
    // Test MilterFilter as part of a filter pipeline
    let configs = vec![
        FilterConfig {
            name: "HeaderFilter".to_string(),
            parameters: json!({}),
        },
        FilterConfig {
            name: "SizeFilter".to_string(),
            parameters: json!({}),
        },
        FilterConfig {
            name: "MilterFilter".to_string(),
            parameters: json!({
                "address": "127.0.0.1:8888",
                "use_tls": false,
                "timeout_secs": 30
            }),
        },
        FilterConfig {
            name: "GroupExistenceFilter".to_string(),
            parameters: json!({}),
        },
    ];

    let chain = create_filter_chain(&configs).unwrap();
    let names = chain.filter_names();

    assert_eq!(names.len(), 4);
    assert_eq!(names[0], "HeaderFilter");
    assert_eq!(names[1], "SizeFilter");
    assert_eq!(names[2], "MilterFilter");
    assert_eq!(names[3], "GroupExistenceFilter");
}

#[tokio::test]
async fn test_config_file_with_milter() {
    let config_content = r#"
addr = ":119"
site_name = "test.example.com"

[milter]
address = "127.0.0.1:8888"
use_tls = false
timeout_secs = 30

[[filters]]
name = "HeaderFilter"

[[filters]]
name = "MilterFilter"
[filters.parameters]
address = "127.0.0.1:8888"
use_tls = false
timeout_secs = 30

[[filters]]
name = "SizeFilter"
"#;

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(config_content.as_bytes()).unwrap();
    temp_file.flush().unwrap();

    let config = Config::from_file(temp_file.path().to_str().unwrap()).unwrap();

    // Test that global milter config is parsed
    assert!(config.milter.is_some());
    let milter_config = config.milter.unwrap();
    assert_eq!(milter_config.address, "127.0.0.1:8888");
    assert!(!milter_config.use_tls);
    assert_eq!(milter_config.timeout_secs, 30);

    // Test that filter pipeline includes MilterFilter
    let chain = create_filter_chain(&config.filters).unwrap();
    let names = chain.filter_names();
    assert_eq!(names.len(), 3);
    assert!(names.contains(&"MilterFilter"));
}

#[tokio::test]
async fn test_milter_filter_invalid_config() {
    // Test error handling for invalid MilterFilter configuration
    let invalid_config = FilterConfig {
        name: "MilterFilter".to_string(),
        parameters: json!({
            "invalid_field": "value"
        }),
    };

    let configs = vec![invalid_config];
    let result = create_filter_chain(&configs);

    assert!(result.is_err());
}
