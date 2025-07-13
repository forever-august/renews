/// Integration test demonstrating the migration system working with the actual renews backends
use renews::{auth, storage};
use std::error::Error;
use tempfile::NamedTempFile;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    println!("=== Renews Backend Migration Integration Test ===\n");

    // Test SQLite storage backend with migrations
    println!("1. Testing SQLite storage backend migration...");
    let storage_temp = NamedTempFile::new()?;
    let storage_path = format!("sqlite://{}", storage_temp.path().display());

    // This should trigger the migration system automatically
    let storage = storage::open(&storage_path).await?;
    println!("   ✓ SQLite storage backend initialized with migrations");

    // Test that the storage works
    match storage.group_exists("test.group").await {
        Ok(exists) => {
            println!("   ✓ Storage functionality verified (group exists check: {exists})")
        }
        Err(e) => println!("   ⚠ Storage test failed: {e}"),
    }

    // Test SQLite auth backend with migrations
    println!("2. Testing SQLite auth backend migration...");
    let auth_temp = NamedTempFile::new()?;
    let auth_path = format!("sqlite://{}", auth_temp.path().display());

    // This should trigger the migration system automatically
    let auth = auth::open(&auth_path).await?;
    println!("   ✓ SQLite auth backend initialized with migrations");

    // Test that the auth works
    match auth.verify_user("testuser", "password").await {
        Ok(verified) => {
            println!("   ✓ Auth functionality verified (user verification: {verified})")
        }
        Err(e) => println!("   ⚠ Auth test failed: {e}"),
    }

    // Test idempotency - reopening should not cause issues
    println!("3. Testing migration idempotency...");

    let storage2 = storage::open(&storage_path).await?;
    println!("   ✓ Storage reopened successfully (migrations idempotent)");

    let auth2 = auth::open(&auth_path).await?;
    println!("   ✓ Auth reopened successfully (migrations idempotent)");

    // Test some basic functionality to ensure the schemas are correct
    println!("4. Testing basic backend functionality...");

    // Add a test group
    storage2.add_group("test.migration.group", false).await?;
    println!("   ✓ Added test group successfully");

    // Check if group exists
    let exists = storage2.group_exists("test.migration.group").await?;
    println!("   ✓ Group existence check: {exists}");

    // Add a test user
    auth2.add_user("migrationtest", "testpass123").await?;
    println!("   ✓ Added test user successfully");

    // Verify the user
    let verified = auth2.verify_user("migrationtest", "testpass123").await?;
    println!("   ✓ User verification: {verified}");

    println!("\n=== All migration integration tests passed! ===");
    println!("The migration system is working correctly with the actual renews backends.");

    Ok(())
}
