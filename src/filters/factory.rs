//! Filter factory for creating filter chains from configuration
//!
//! This module provides functionality to parse filter configurations and create
//! custom filter chains that can be used instead of the default chain.

use super::{ArticleFilter, FilterChain};
use crate::config::FilterConfig;


/// Errors that can occur when creating filters from configuration
#[derive(Debug, Clone)]
pub enum FilterFactoryError {
    /// Unknown filter name
    UnknownFilter(String),
    /// Invalid parameters for a filter
    InvalidParameters(String),
}

impl std::fmt::Display for FilterFactoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterFactoryError::UnknownFilter(name) => {
                write!(f, "Unknown filter: {name}")
            }
            FilterFactoryError::InvalidParameters(msg) => {
                write!(f, "Invalid filter parameters: {msg}")
            }
        }
    }
}

impl Error for FilterFactoryError {}

/// Create a filter instance from configuration
pub fn create_filter(config: &FilterConfig) -> Result<Box<dyn ArticleFilter>, FilterFactoryError> {
    match config.name.as_str() {
        "HeaderFilter" => Ok(Box::new(super::header::HeaderFilter)),
        "SizeFilter" => Ok(Box::new(super::size::SizeFilter)),
        "GroupExistenceFilter" => Ok(Box::new(super::groups::GroupExistenceFilter)),
        "ModerationFilter" => Ok(Box::new(super::moderation::ModerationFilter)),
        "MilterFilter" => {
            // Extract Milter configuration from parameters
            let milter_config: super::milter::MilterConfig =
                serde_json::from_value(serde_json::Value::Object(config.parameters.clone()))
                    .map_err(|e| {
                        FilterFactoryError::InvalidParameters(format!(
                            "MilterFilter configuration error: {e}"
                        ))
                    })?;
            Ok(Box::new(super::milter::MilterFilter::new(milter_config)))
        }
        _ => Err(FilterFactoryError::UnknownFilter(config.name.clone())),
    }
}

/// Create a filter chain from a list of filter configurations
///
/// If the configuration is empty, returns the default filter chain.
/// Otherwise, creates a custom chain with the specified filters in order.
pub fn create_filter_chain(configs: &[FilterConfig]) -> Result<FilterChain, FilterFactoryError> {
    if configs.is_empty() {
        // If no filter configuration is provided, use the default chain
        return Ok(FilterChain::default());
    }

    let mut chain = FilterChain::new();
    for config in configs {
        let filter = create_filter(config)?;
        chain = chain.add_filter(filter);
    }
    Ok(chain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_create_header_filter() {
        let config = FilterConfig {
            name: "HeaderFilter".to_string(),
            parameters: serde_json::Map::new(),
        };

        let filter = create_filter(&config).unwrap();
        assert_eq!(filter.name(), "HeaderFilter");
    }

    #[test]
    fn test_create_size_filter() {
        let config = FilterConfig {
            name: "SizeFilter".to_string(),
            parameters: serde_json::Map::new(),
        };

        let filter = create_filter(&config).unwrap();
        assert_eq!(filter.name(), "SizeFilter");
    }

    #[test]
    fn test_create_group_existence_filter() {
        let config = FilterConfig {
            name: "GroupExistenceFilter".to_string(),
            parameters: serde_json::Map::new(),
        };

        let filter = create_filter(&config).unwrap();
        assert_eq!(filter.name(), "GroupExistenceFilter");
    }

    #[test]
    fn test_create_moderation_filter() {
        let config = FilterConfig {
            name: "ModerationFilter".to_string(),
            parameters: serde_json::Map::new(),
        };

        let filter = create_filter(&config).unwrap();
        assert_eq!(filter.name(), "ModerationFilter");
    }

    #[test]
    fn test_create_milter_filter() {
        let mut parameters = serde_json::Map::new();
        parameters.insert("address".to_string(), json!("tcp://127.0.0.1:8888"));
        parameters.insert("timeout_secs".to_string(), json!(30));

        let config = FilterConfig {
            name: "MilterFilter".to_string(),
            parameters,
        };

        let filter = create_filter(&config).unwrap();
        assert_eq!(filter.name(), "MilterFilter");
    }

    #[test]
    fn test_unknown_filter() {
        let config = FilterConfig {
            name: "UnknownFilter".to_string(),
            parameters: serde_json::Map::new(),
        };

        let result = create_filter(&config);
        assert!(result.is_err());
        if let Err(FilterFactoryError::UnknownFilter(name)) = result {
            assert_eq!(name, "UnknownFilter");
        } else {
            panic!("Expected UnknownFilter error");
        }
    }

    #[test]
    fn test_create_empty_filter_chain() {
        let configs = vec![];
        let chain = create_filter_chain(&configs).unwrap();
        // Default chain should have 4 filters
        assert_eq!(chain.filter_names().len(), 4);
    }

    #[test]
    fn test_create_custom_filter_chain() {
        let configs = vec![
            FilterConfig {
                name: "HeaderFilter".to_string(),
                parameters: serde_json::Map::new(),
            },
            FilterConfig {
                name: "SizeFilter".to_string(),
                parameters: serde_json::Map::new(),
            },
        ];

        let chain = create_filter_chain(&configs).unwrap();
        let names = chain.filter_names();
        assert_eq!(names.len(), 2);
        assert_eq!(names[0], "HeaderFilter");
        assert_eq!(names[1], "SizeFilter");
    }

    #[test]
    fn test_create_filter_chain_with_unknown_filter() {
        let configs = vec![
            FilterConfig {
                name: "HeaderFilter".to_string(),
                parameters: serde_json::Map::new(),
            },
            FilterConfig {
                name: "UnknownFilter".to_string(),
                parameters: serde_json::Map::new(),
            },
        ];

        let result = create_filter_chain(&configs);
        assert!(result.is_err());
    }
}
