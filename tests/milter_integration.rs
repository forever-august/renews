//! Integration tests for Milter filter configuration and usage

use renews::config::{Config, FilterConfig};
use renews::filters::factory::create_filter_chain;
use serde_json::json;
use std::io::Write;
use tempfile::NamedTempFile;

#[tokio::test]
async fn test_milter_filter_configuration() {
    // Test creating a filter chain with MilterFilter
    let mut parameters = serde_json::Map::new();
    parameters.insert("address".to_string(), json!("tcp://127.0.0.1:8888"));
    parameters.insert("timeout_secs".to_string(), json!(30));

    let milter_config = FilterConfig {
        name: "MilterFilter".to_string(),
        parameters,
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
    let mut parameters = serde_json::Map::new();
    parameters.insert(
        "address".to_string(),
        json!("tls://milter.example.com:8888"),
    );
    parameters.insert("timeout_secs".to_string(), json!(60));

    let milter_config = FilterConfig {
        name: "MilterFilter".to_string(),
        parameters,
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
    let mut milter_parameters = serde_json::Map::new();
    milter_parameters.insert("address".to_string(), json!("tcp://127.0.0.1:8888"));
    milter_parameters.insert("timeout_secs".to_string(), json!(30));

    let configs = vec![
        FilterConfig {
            name: "HeaderFilter".to_string(),
            parameters: serde_json::Map::new(),
        },
        FilterConfig {
            name: "SizeFilter".to_string(),
            parameters: serde_json::Map::new(),
        },
        FilterConfig {
            name: "MilterFilter".to_string(),
            parameters: milter_parameters,
        },
        FilterConfig {
            name: "GroupExistenceFilter".to_string(),
            parameters: serde_json::Map::new(),
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

[[filters]]
name = "HeaderFilter"

[[filters]]
name = "MilterFilter"
address = "tcp://127.0.0.1:8888"
timeout_secs = 30

[[filters]]
name = "SizeFilter"
"#;

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(config_content.as_bytes()).unwrap();
    temp_file.flush().unwrap();

    let config = Config::from_file(temp_file.path().to_str().unwrap()).unwrap();

    // Test that filter pipeline includes MilterFilter
    let chain = create_filter_chain(&config.filters).unwrap();
    let names = chain.filter_names();
    assert_eq!(names.len(), 3);
    assert!(names.contains(&"MilterFilter"));

    // Test that the MilterFilter has the correct configuration
    assert_eq!(config.filters.len(), 3);
    let milter_filter = config
        .filters
        .iter()
        .find(|f| f.name == "MilterFilter")
        .unwrap();
    assert_eq!(
        milter_filter.parameters.get("address").unwrap(),
        "tcp://127.0.0.1:8888"
    );
    assert_eq!(milter_filter.parameters.get("timeout_secs").unwrap(), 30);
}

#[tokio::test]
async fn test_milter_filter_invalid_config() {
    // Test error handling for invalid MilterFilter configuration
    let mut invalid_parameters = serde_json::Map::new();
    invalid_parameters.insert("invalid_field".to_string(), json!("value"));

    let invalid_config = FilterConfig {
        name: "MilterFilter".to_string(),
        parameters: invalid_parameters,
    };

    let configs = vec![invalid_config];
    let result = create_filter_chain(&configs);

    assert!(result.is_err());
}

#[tokio::test]
async fn test_milter_filter_with_unix_socket_configuration() {
    // Test creating a filter chain with MilterFilter using Unix socket
    let mut parameters = serde_json::Map::new();
    parameters.insert("address".to_string(), json!("unix:///var/run/milter.sock"));
    parameters.insert("timeout_secs".to_string(), json!(30));

    let milter_config = FilterConfig {
        name: "MilterFilter".to_string(),
        parameters,
    };

    let configs = vec![milter_config];
    let chain = create_filter_chain(&configs).unwrap();
    let names = chain.filter_names();

    assert_eq!(names.len(), 1);
    assert_eq!(names[0], "MilterFilter");
}
