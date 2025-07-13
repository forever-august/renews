//! Integration tests for filter pipeline configuration

use renews::config::{Config, FilterConfig};
use renews::filters::factory::create_filter_chain;
use serde_json::json;
use tempfile::NamedTempFile;

#[tokio::test]
async fn test_filter_pipeline_configuration() {
    // Test empty filter pipeline (should use default)
    let empty_config = vec![];
    let chain = create_filter_chain(&empty_config).unwrap();
    assert_eq!(chain.filter_names().len(), 4); // Default chain has 4 filters

    // Test custom filter pipeline
    let custom_config = vec![
        FilterConfig {
            name: "HeaderFilter".to_string(),
            parameters: serde_json::Map::new(),
        },
        FilterConfig {
            name: "SizeFilter".to_string(),
            parameters: serde_json::Map::new(),
        },
    ];
    let chain = create_filter_chain(&custom_config).unwrap();
    let names = chain.filter_names();
    assert_eq!(names.len(), 2);
    assert_eq!(names[0], "HeaderFilter");
    assert_eq!(names[1], "SizeFilter");
}

#[tokio::test]
async fn test_config_file_with_filter_pipeline() {
    let config_content = r#"
addr = ":119"
site_name = "test.example.com"

[[filters]]
name = "HeaderFilter"

[[filters]]
name = "SizeFilter"

[[filters]]
name = "GroupExistenceFilter"
"#;

    let mut temp_file = NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut temp_file, config_content.as_bytes()).unwrap();

    let config = Config::from_file(temp_file.path().to_str().unwrap()).unwrap();

    // Check that filter pipeline was parsed correctly
    assert_eq!(config.filters.len(), 3);
    assert_eq!(config.filters[0].name, "HeaderFilter");
    assert_eq!(config.filters[1].name, "SizeFilter");
    assert_eq!(config.filters[2].name, "GroupExistenceFilter");

    // Test creating filter chain from this config
    let chain = create_filter_chain(&config.filters).unwrap();
    let names = chain.filter_names();
    assert_eq!(names.len(), 3);
    assert_eq!(names[0], "HeaderFilter");
    assert_eq!(names[1], "SizeFilter");
    assert_eq!(names[2], "GroupExistenceFilter");
}

#[tokio::test]
async fn test_config_reload_updates_filter_pipeline() {
    // Create initial config
    let initial_config_content = r#"
addr = ":119"
site_name = "test.example.com"

[[filters]]
name = "HeaderFilter"

[[filters]]
name = "SizeFilter"
"#;

    let mut temp_file = NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut temp_file, initial_config_content.as_bytes()).unwrap();

    let mut config = Config::from_file(temp_file.path().to_str().unwrap()).unwrap();
    assert_eq!(config.filters.len(), 2);

    // Create new config with different filter pipeline
    let new_config_content = r#"
addr = ":119"
site_name = "test.example.com"

[[filters]]
name = "HeaderFilter"

[[filters]]
name = "SizeFilter"

[[filters]]
name = "GroupExistenceFilter"

[[filters]]
name = "ModerationFilter"
"#;

    let mut new_temp_file = NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut new_temp_file, new_config_content.as_bytes()).unwrap();

    let new_config = Config::from_file(new_temp_file.path().to_str().unwrap()).unwrap();

    // Update runtime config
    config.update_runtime(new_config);

    // Verify that filter pipeline was updated
    assert_eq!(config.filters.len(), 4);
    assert_eq!(config.filters[0].name, "HeaderFilter");
    assert_eq!(config.filters[1].name, "SizeFilter");
    assert_eq!(config.filters[2].name, "GroupExistenceFilter");
    assert_eq!(config.filters[3].name, "ModerationFilter");

    // Test that the new filter chain can be created
    let chain = create_filter_chain(&config.filters).unwrap();
    let names = chain.filter_names();
    assert_eq!(names.len(), 4);
    assert_eq!(names[0], "HeaderFilter");
    assert_eq!(names[1], "SizeFilter");
    assert_eq!(names[2], "GroupExistenceFilter");
    assert_eq!(names[3], "ModerationFilter");
}

#[tokio::test]
async fn test_filter_config_with_parameters() {
    // Test that filter configurations with parameters are parsed correctly
    let config_content = r#"
addr = ":119"
site_name = "test.example.com"

[[filters]]
name = "HeaderFilter"

[[filters]]
name = "SizeFilter"
max_size = 1048576
"#;

    let mut temp_file = NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut temp_file, config_content.as_bytes()).unwrap();

    let config = Config::from_file(temp_file.path().to_str().unwrap()).unwrap();

    // Check that parameters were parsed correctly
    assert_eq!(config.filters.len(), 2);
    assert_eq!(config.filters[0].name, "HeaderFilter");
    // Default parameters should be empty when not specified
    assert!(config.filters[0].parameters.is_empty());
    assert_eq!(config.filters[1].name, "SizeFilter");
    assert_eq!(
        config.filters[1].parameters.get("max_size").unwrap(),
        &json!(1048576)
    );
}

#[tokio::test]
async fn test_invalid_filter_fallback() {
    // Test that invalid filter configurations fall back gracefully
    let config_with_invalid_filter = vec![
        FilterConfig {
            name: "HeaderFilter".to_string(),
            parameters: serde_json::Map::new(),
        },
        FilterConfig {
            name: "InvalidFilter".to_string(),
            parameters: serde_json::Map::new(),
        },
    ];

    let result = create_filter_chain(&config_with_invalid_filter);
    assert!(result.is_err());

    // The error should be handled gracefully in the queue processing,
    // falling back to the default filter chain
}

#[test]
fn test_filters_alias() {
    // Test with [[filters]] syntax
    let config_content_filters = r#"
addr = ":119"
site_name = "test.example.com"

[[filters]]
name = "HeaderFilter"

[[filters]]
name = "SizeFilter"
"#;

    let mut temp_file1 = NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut temp_file1, config_content_filters.as_bytes()).unwrap();

    let config1 = Config::from_file(temp_file1.path().to_str().unwrap()).unwrap();
    assert_eq!(config1.filters.len(), 2);

    // Test with [[filter]] syntax (using alias)
    let config_content_filter = r#"
addr = ":119"
site_name = "test.example.com"

[[filter]]
name = "HeaderFilter"

[[filter]]
name = "SizeFilter"
"#;

    let mut temp_file2 = NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut temp_file2, config_content_filter.as_bytes()).unwrap();

    let config2 = Config::from_file(temp_file2.path().to_str().unwrap()).unwrap();
    assert_eq!(config2.filters.len(), 2);

    // Both should parse the same way
    assert_eq!(config1.filters[0].name, config2.filters[0].name);
    assert_eq!(config1.filters[1].name, config2.filters[1].name);
}
