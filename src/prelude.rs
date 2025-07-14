//! Prelude module that provides common traits and type conversions
//! 
//! This module should be imported as `use crate::prelude::*` in modules that need
//! to convert between different error types.

use anyhow::Result;

/// Extension trait to convert Box<dyn Error + Send + Sync> to anyhow::Error
pub trait AnyhowExt<T> {
    fn to_anyhow(self) -> Result<T>;
}

impl<T> AnyhowExt<T> for std::result::Result<T, Box<dyn std::error::Error + Send + Sync>> {
    fn to_anyhow(self) -> Result<T> {
        self.map_err(anyhow::Error::from)
    }
}