use renews::auth::{AuthProvider, sqlite::SqliteAuth};

#[tokio::test]
async fn add_and_check_admin() {
    let auth = SqliteAuth::new("sqlite::memory:").await.unwrap();
    auth.add_user("user", "pass").await.unwrap();
    assert!(!auth.is_admin("user").await.unwrap());
    auth.add_admin("user").await.unwrap();
    assert!(auth.is_admin("user").await.unwrap());
    auth.remove_admin("user").await.unwrap();
    assert!(!auth.is_admin("user").await.unwrap());
}

#[tokio::test]
async fn add_and_remove_user() {
    let auth = SqliteAuth::new("sqlite::memory:").await.unwrap();
    auth.add_user("user", "pass").await.unwrap();
    assert!(auth.verify_user("user", "pass").await.unwrap());
    auth.remove_user("user").await.unwrap();
    assert!(!auth.verify_user("user", "pass").await.unwrap());
}
