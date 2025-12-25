//! Prelude module that provides common imports
//!
//! This module should be imported as `use crate::prelude::*` in modules that need
//! common functionality.

// Re-export anyhow::Result as the standard Result type for the crate
pub use anyhow::Result;

// Re-export Session for convenient access
pub use crate::session::Session;
