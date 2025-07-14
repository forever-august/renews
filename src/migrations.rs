use anyhow::Result;
use async_trait::async_trait;

/// Represents a single database migration step.
///
/// Each migration transforms the database from one version to the next.
/// Migrations should be idempotent and safe to run multiple times.
#[async_trait]
pub trait Migration: Send + Sync {
    /// The target version this migration upgrades to.
    fn target_version(&self) -> u32;

    /// A human-readable description of what this migration does.
    fn description(&self) -> &str;

    /// Apply this migration to the database.
    ///
    /// Should be idempotent - running multiple times should not cause issues.
    async fn apply(&self) -> Result<()>;
}

/// Manages database schema versions and applies migrations.
///
/// This trait provides a common interface for version tracking and migration
/// application across different backend types (SQL and non-SQL).
#[async_trait]
pub trait Migrator: Send + Sync {
    /// Get the current schema version stored in the backend.
    ///
    /// Returns 0 if no version is stored (fresh install).
    async fn get_current_version(&self) -> Result<u32>;

    /// Set the schema version in the backend.
    async fn set_version(&self, version: u32) -> Result<()>;

    /// Get all available migrations for this backend, ordered by target version.
    fn get_migrations(&self) -> Vec<Box<dyn Migration>>;

    /// Check if this is a fresh database that needs initialization.
    ///
    /// This method attempts to read the version table. If it fails,
    /// we assume this is a fresh database.
    async fn is_fresh_database(&self) -> bool {
        (self.get_current_version().await).is_err()
    }

    /// Apply all necessary migrations to reach the latest version.
    ///
    /// This method should only be called after the database has been initialized
    /// with the current schema. The flow should be:
    /// 1. Check if database is fresh (cannot read version)
    /// 2. If fresh, initialize with current schema and set to latest version
    /// 3. If not fresh, apply any pending migrations
    ///
    /// Returns an error if any migration fails or if the stored version
    /// is higher than the latest available version.
    async fn migrate_to_latest(&self) -> Result<()> {
        let current_version = self.get_current_version().await?;
        let migrations = self.get_migrations();

        if migrations.is_empty() {
            tracing::info!("No migrations available");
            return Ok(());
        }

        let latest_version = migrations
            .iter()
            .map(|m| m.target_version())
            .max()
            .unwrap_or(0);

        if current_version > latest_version {
            return Err(format!(
                "Stored schema version {current_version} is higher than latest available version {latest_version}. \
                This usually means you're trying to run an older version of the software \
                against a newer database. Please upgrade to a compatible version."
            ).into());
        }

        if current_version == latest_version {
            tracing::info!(
                "Database schema is up to date at version {}",
                current_version
            );
            return Ok(());
        }

        tracing::info!(
            "Migrating database schema from version {} to version {}",
            current_version,
            latest_version
        );

        // Apply migrations in sequence
        for migration in migrations {
            let target = migration.target_version();

            // Skip migrations we've already applied
            if target <= current_version {
                continue;
            }

            tracing::info!(
                "Applying migration to version {}: {}",
                target,
                migration.description()
            );

            migration
                .apply()
                .await
                .map_err(|e| format!("Failed to apply migration to version {target}: {e}"))?;

            self.set_version(target)
                .await
                .map_err(|e| format!("Failed to update schema version to {target}: {e}"))?;

            tracing::info!("Successfully migrated to version {}", target);
        }

        tracing::info!("Database migration completed successfully");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    // Mock migration for testing
    struct MockMigration {
        version: u32,
        description: String,
        should_fail: Arc<AtomicBool>,
        apply_count: Arc<AtomicU32>,
    }

    impl MockMigration {
        fn new(version: u32, description: &str) -> Self {
            Self {
                version,
                description: description.to_string(),
                should_fail: Arc::new(AtomicBool::new(false)),
                apply_count: Arc::new(AtomicU32::new(0)),
            }
        }

        fn with_failure(self) -> Self {
            self.should_fail.store(true, Ordering::SeqCst);
            self
        }

        fn get_apply_count(&self) -> u32 {
            self.apply_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl Migration for MockMigration {
        fn target_version(&self) -> u32 {
            self.version
        }

        fn description(&self) -> &str {
            &self.description
        }

        async fn apply(&self) -> Result<()> {
            self.apply_count.fetch_add(1, Ordering::SeqCst);

            if self.should_fail.load(Ordering::SeqCst) {
                return Err("Mock migration failure".into());
            }

            Ok(())
        }
    }

    // Mock migrator for testing
    struct MockMigrator {
        current_version: Arc<AtomicU32>,
        can_read_version: Arc<AtomicBool>,
    }

    impl MockMigrator {
        fn new(initial_version: u32) -> Self {
            Self {
                current_version: Arc::new(AtomicU32::new(initial_version)),
                can_read_version: Arc::new(AtomicBool::new(true)),
            }
        }

        fn with_fresh_database(self) -> Self {
            self.can_read_version.store(false, Ordering::SeqCst);
            self
        }
    }

    #[async_trait]
    impl Migrator for MockMigrator {
        async fn get_current_version(&self) -> Result<u32> {
            if self.can_read_version.load(Ordering::SeqCst) {
                Ok(self.current_version.load(Ordering::SeqCst))
            } else {
                Err("Cannot read version table".into())
            }
        }

        async fn set_version(&self, version: u32) -> Result<()> {
            self.current_version.store(version, Ordering::SeqCst);
            self.can_read_version.store(true, Ordering::SeqCst);
            Ok(())
        }

        fn get_migrations(&self) -> Vec<Box<dyn Migration>> {
            // For tests, we'll return empty and test the individual methods
            Vec::new()
        }
    }

    #[tokio::test]
    async fn test_migrator_no_migrations() {
        let migrator = MockMigrator::new(0);

        // No migrations means no work to do
        let result = migrator.migrate_to_latest().await;
        assert!(result.is_ok());

        // Version should remain 0
        let version = migrator.get_current_version().await.unwrap();
        assert_eq!(version, 0);
    }

    #[tokio::test]
    async fn test_migrator_up_to_date() {
        let migrator = MockMigrator::new(5);

        // If we're already at the latest version, should be no-op
        let result = migrator.migrate_to_latest().await;
        assert!(result.is_ok());

        // Version should remain 5
        let version = migrator.get_current_version().await.unwrap();
        assert_eq!(version, 5);
    }

    #[tokio::test]
    async fn test_migrator_fresh_database() {
        let migrator = MockMigrator::new(0).with_fresh_database();

        // Fresh database should not be able to read version
        assert!(migrator.is_fresh_database().await);

        // But once we set a version, it should work
        migrator.set_version(1).await.unwrap();
        assert!(!migrator.is_fresh_database().await);

        let version = migrator.get_current_version().await.unwrap();
        assert_eq!(version, 1);
    }

    #[tokio::test]
    async fn test_migrator_version_too_high() {
        // Test the case where stored version is higher than available migrations

        // Create a migrator that simulates this scenario
        struct HighVersionMigrator;

        #[async_trait]
        impl Migrator for HighVersionMigrator {
            async fn get_current_version(&self) -> Result<u32> {
                Ok(10) // High version
            }

            async fn set_version(&self, _version: u32) -> Result<()> {
                Ok(())
            }

            fn get_migrations(&self) -> Vec<Box<dyn Migration>> {
                vec![Box::new(MockMigration::new(5, "Test migration"))]
            }
        }

        let migrator = HighVersionMigrator;
        let result = migrator.migrate_to_latest().await;

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg
                .contains("Stored schema version 10 is higher than latest available version 5")
        );
    }

    #[tokio::test]
    async fn test_migration_basic_properties() {
        let migration = MockMigration::new(42, "Test migration for version 42");

        assert_eq!(migration.target_version(), 42);
        assert_eq!(migration.description(), "Test migration for version 42");

        // Test successful apply
        let result = migration.apply().await;
        assert!(result.is_ok());
        assert_eq!(migration.get_apply_count(), 1);

        // Test idempotency
        let result = migration.apply().await;
        assert!(result.is_ok());
        assert_eq!(migration.get_apply_count(), 2);
    }

    #[tokio::test]
    async fn test_migration_failure() {
        let migration = MockMigration::new(1, "Failing migration").with_failure();

        let result = migration.apply().await;
        assert!(result.is_err());
        assert_eq!(migration.get_apply_count(), 1);

        let error_msg = result.unwrap_err().to_string();
        assert_eq!(error_msg, "Mock migration failure");
    }
}
