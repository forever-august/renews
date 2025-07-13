# Database Migration System

This document describes the database migration system implemented in Renews. The migration system provides automatic schema versioning and upgrading for all storage and authentication backends.

## Overview

The migration system ensures that:
- Each backend tracks its own schema version
- Schema upgrades are applied automatically on startup
- Migrations are idempotent and safe to run multiple times
- Future schema changes can be easily added
- The system works with both SQL and non-SQL backends

## Architecture

### Core Components

1. **`Migration` trait** - Represents a single migration step
2. **`Migrator` trait** - Manages version tracking and migration execution
3. **Backend-specific implementations** - SQLite and PostgreSQL implementations for storage and auth

### Migration Flow

1. Backend initialization creates schema version table if needed
2. Current version is read from storage (0 if none exists)
3. Available migrations are collected and sorted by target version
4. If stored version > latest available version, error is returned
5. If stored version < latest available version, missing migrations are applied sequentially
6. Version is updated after each successful migration

## Adding New Migrations

### For Storage Backends

1. **Create the migration struct** in `src/migrations/storage.rs`:

```rust
pub struct SqliteStorageMigration002 {
    pool: SqlitePool,
}

impl SqliteStorageMigration002 {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Migration for SqliteStorageMigration002 {
    fn target_version(&self) -> u32 {
        2  // Increment from previous version
    }
    
    fn description(&self) -> &str {
        "Add index on group_articles.inserted_at for performance"
    }
    
    async fn apply(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Apply the schema change
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_group_articles_inserted_at ON group_articles(inserted_at)")
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
}
```

2. **Add the migration to the migrator** in the `get_migrations()` method:

```rust
fn get_migrations(&self) -> Vec<Box<dyn Migration>> {
    vec![
        Box::new(SqliteStorageMigration001::new(self.pool.clone())),
        Box::new(SqliteStorageMigration002::new(self.pool.clone())),
    ]
}
```

3. **Add tests** for the new migration:

```rust
#[tokio::test]
async fn test_sqlite_storage_migration_002() {
    // Set up test database
    // Create the migration
    // Test that it applies successfully
    // Test that it's idempotent
}
```

### For Auth Backends

Follow the same pattern in `src/migrations/auth.rs` for authentication schema changes.

### For PostgreSQL

PostgreSQL migrations follow the same pattern but use `sqlx::PgPool` instead of `SqlitePool`.

## Migration Guidelines

### Writing Safe Migrations

1. **Always use `IF NOT EXISTS`** when creating tables or indexes
2. **Always use `IF EXISTS`** when dropping tables or columns
3. **Test migrations thoroughly** with existing data
4. **Make migrations idempotent** - running twice should not cause errors
5. **Use transactions** for complex migrations to ensure atomicity

### Version Numbering

- Each backend maintains its own version sequence
- Storage backend: 1, 2, 3, ...
- Auth backend: 1, 2, 3, ...
- Never reuse or skip version numbers
- Never change existing migrations once released

### Example Safe Migration Patterns

```rust
// ✅ Safe - creates index only if it doesn't exist
sqlx::query("CREATE INDEX IF NOT EXISTS idx_messages_date ON messages(created_at)")
    .execute(&self.pool)
    .await?;

// ✅ Safe - adds column with default value
sqlx::query("ALTER TABLE messages ADD COLUMN priority INTEGER DEFAULT 0")
    .execute(&self.pool)
    .await?;

// ❌ Unsafe - will fail if index already exists
sqlx::query("CREATE INDEX idx_messages_date ON messages(created_at)")
    .execute(&self.pool)
    .await?;

// ❌ Unsafe - will fail if column already exists
sqlx::query("ALTER TABLE messages ADD COLUMN priority INTEGER")
    .execute(&self.pool)
    .await?;
```

## Testing Migrations

### Unit Tests

Each migration should have unit tests that verify:
- Migration applies successfully
- Migration is idempotent (can run multiple times)
- Schema changes are correctly applied

### Integration Tests

Test the full migration flow:
- Fresh installation (version 0 to latest)
- Partial upgrades (version N to latest)
- Error handling (invalid migrations, database errors)

### Example Test Structure

```rust
#[tokio::test]
async fn test_storage_migration_full_flow() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = format!("sqlite://{}", temp_file.path().display());
    let pool = sqlx::SqlitePool::connect(&db_path).await.unwrap();
    
    // Create baseline schema
    // ... create tables ...
    
    let migrator = SqliteStorageMigrator::new(pool.clone());
    
    // Test fresh install
    assert_eq!(migrator.get_current_version().await.unwrap(), 0);
    migrator.migrate_to_latest().await.unwrap();
    assert_eq!(migrator.get_current_version().await.unwrap(), 2); // Latest version
    
    // Test idempotency
    migrator.migrate_to_latest().await.unwrap();
    assert_eq!(migrator.get_current_version().await.unwrap(), 2);
}
```

## Non-SQL Backends

The migration system is designed to work with non-SQL backends as well. For example, a file-based backend might:

1. Store version in a `.version` file
2. Apply migrations by reorganizing directory structure
3. Transform data files between formats

The `Migration` and `Migrator` traits provide the necessary abstraction for any backend type.

## Error Handling

The migration system handles several error conditions:

1. **Database connection errors** - Propagated to caller
2. **Migration application errors** - Migration stops, version not updated
3. **Version downgrade attempts** - Error returned, no migrations applied
4. **Corrupted version table** - Error returned, manual intervention required

## Logging

The migration system provides comprehensive logging:

- `INFO` - Migration start/completion, version changes
- `DEBUG` - Individual migration details
- `ERROR` - Migration failures, version conflicts

Enable logging to monitor migration progress:

```rust
RUST_LOG=renews::migrations=info cargo run
```

## Manual Migration Recovery

If migrations fail or the version table becomes corrupted:

1. **Backup the database** before any manual intervention
2. **Check the schema_version table** to see the recorded version
3. **Manually fix any schema issues** if possible
4. **Update the version** to match the actual schema state
5. **Test migrations** on a copy before applying to production

Example manual version update:

```sql
-- SQLite
DELETE FROM schema_version;
INSERT INTO schema_version (version) VALUES (2);

-- PostgreSQL
DELETE FROM schema_version;
INSERT INTO schema_version (version) VALUES (2);
```

## Best Practices

1. **Always backup** before running migrations in production
2. **Test migrations** on a copy of production data
3. **Monitor logs** during migration execution
4. **Plan for rollback** if migrations fail
5. **Document breaking changes** in migration descriptions
6. **Coordinate deployments** when schema changes affect multiple services

## Future Enhancements

Potential future improvements to the migration system:

1. **Rollback support** - Allow downgrading to previous versions
2. **Migration validation** - Check migrations before applying
3. **Dry-run mode** - Preview what migrations would do
4. **Parallel migrations** - Apply independent migrations concurrently
5. **Cross-backend dependencies** - Coordinate migrations between backends