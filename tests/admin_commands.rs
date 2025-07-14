use renews::{auth, storage};
use tempfile::TempDir;

async fn setup() -> (String, String, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let storage_path = format!("sqlite:///{}/storage.db", temp_dir.path().to_str().unwrap());
    let auth_path = format!("sqlite:///{}/auth.db", temp_dir.path().to_str().unwrap());
    
    // Initialize databases
    storage::open(&storage_path).await.unwrap();
    auth::open(&auth_path).await.unwrap();
    
    (storage_path, auth_path, temp_dir)
}

#[tokio::test]
async fn test_add_user_with_pgp_key() {
    let (_storage_path, auth_path, _temp_dir) = setup().await;
    let auth = auth::open(&auth_path).await.unwrap();
    
    // Test adding user with PGP key
    auth.add_user_with_key("testuser", "testpass", Some("test-pgp-key")).await.unwrap();
    
    // Verify user can authenticate
    assert!(auth.verify_user("testuser", "testpass").await.unwrap());
    
    // Verify PGP key is stored
    let key = auth.get_pgp_key("testuser").await.unwrap();
    assert_eq!(key, Some("test-pgp-key".to_string()));
}

#[tokio::test]
async fn test_add_user_without_pgp_key() {
    let (_storage_path, auth_path, _temp_dir) = setup().await;
    let auth = auth::open(&auth_path).await.unwrap();
    
    // Test adding user without PGP key
    auth.add_user_with_key("testuser", "testpass", None).await.unwrap();
    
    // Verify user can authenticate
    assert!(auth.verify_user("testuser", "testpass").await.unwrap());
    
    // Verify no PGP key is stored
    let key = auth.get_pgp_key("testuser").await.unwrap();
    assert_eq!(key, None);
}

#[tokio::test]
async fn test_update_password() {
    let (_storage_path, auth_path, _temp_dir) = setup().await;
    let auth = auth::open(&auth_path).await.unwrap();
    
    // Add user
    auth.add_user("testuser", "oldpass").await.unwrap();
    
    // Verify old password works
    assert!(auth.verify_user("testuser", "oldpass").await.unwrap());
    
    // Update password
    auth.update_password("testuser", "newpass").await.unwrap();
    
    // Verify old password no longer works
    assert!(!auth.verify_user("testuser", "oldpass").await.unwrap());
    
    // Verify new password works
    assert!(auth.verify_user("testuser", "newpass").await.unwrap());
}

#[tokio::test]
async fn test_add_admin_without_key() {
    let (_storage_path, auth_path, _temp_dir) = setup().await;
    let auth = auth::open(&auth_path).await.unwrap();
    
    // Add user
    auth.add_user("testuser", "testpass").await.unwrap();
    
    // User should not be admin initially
    assert!(!auth.is_admin("testuser").await.unwrap());
    
    // Make user admin without key
    auth.add_admin_without_key("testuser").await.unwrap();
    
    // User should now be admin
    assert!(auth.is_admin("testuser").await.unwrap());
    
    // Should not have a PGP key
    let key = auth.get_pgp_key("testuser").await.unwrap();
    assert_eq!(key, None);
}

#[tokio::test]
async fn test_set_group_moderated() {
    let (storage_path, _auth_path, _temp_dir) = setup().await;
    let storage = storage::open(&storage_path).await.unwrap();
    
    // Add a group
    storage.add_group("test.group", false).await.unwrap();
    
    // Verify it's not moderated
    assert!(!storage.is_group_moderated("test.group").await.unwrap());
    
    // Set as moderated
    storage.set_group_moderated("test.group", true).await.unwrap();
    
    // Verify it's now moderated
    assert!(storage.is_group_moderated("test.group").await.unwrap());
    
    // Set as not moderated
    storage.set_group_moderated("test.group", false).await.unwrap();
    
    // Verify it's not moderated
    assert!(!storage.is_group_moderated("test.group").await.unwrap());
}

#[tokio::test]
async fn test_remove_groups_by_pattern() {
    let (storage_path, _auth_path, _temp_dir) = setup().await;
    let storage = storage::open(&storage_path).await.unwrap();
    
    // Add multiple groups
    storage.add_group("test.group1", false).await.unwrap();
    storage.add_group("test.group2", false).await.unwrap();
    storage.add_group("other.group", false).await.unwrap();
    
    // Verify all groups exist
    assert!(storage.group_exists("test.group1").await.unwrap());
    assert!(storage.group_exists("test.group2").await.unwrap());
    assert!(storage.group_exists("other.group").await.unwrap());
    
    // Remove groups matching pattern "test.*"
    storage.remove_groups_by_pattern("test.*").await.unwrap();
    
    // Verify test groups are removed but other group remains
    assert!(!storage.group_exists("test.group1").await.unwrap());
    assert!(!storage.group_exists("test.group2").await.unwrap());
    assert!(storage.group_exists("other.group").await.unwrap());
}