/// SQL schemas and common utilities for authentication implementations.

/// SQL schemas for authentication tables.
pub mod sql {
    /// Schema for the users table (stores user credentials and PGP keys).
    pub const USERS_TABLE_SQLITE: &str = 
        "CREATE TABLE IF NOT EXISTS users (
            username TEXT PRIMARY KEY,
            password_hash TEXT NOT NULL,
            key TEXT
        )";

    pub const USERS_TABLE_POSTGRES: &str = 
        "CREATE TABLE IF NOT EXISTS users (
            username TEXT PRIMARY KEY,
            password_hash TEXT NOT NULL,
            key TEXT
        )";

    /// Schema for the admins table (stores admin users).
    pub const ADMINS_TABLE_SQLITE: &str = 
        "CREATE TABLE IF NOT EXISTS admins (
            username TEXT PRIMARY KEY REFERENCES users(username)
        )";

    pub const ADMINS_TABLE_POSTGRES: &str = 
        "CREATE TABLE IF NOT EXISTS admins (
            username TEXT PRIMARY KEY REFERENCES users(username)
        )";

    /// Schema for the moderators table (stores moderator privileges by pattern).
    pub const MODERATORS_TABLE_SQLITE: &str = 
        "CREATE TABLE IF NOT EXISTS moderators (
            username TEXT REFERENCES users(username),
            pattern TEXT,
            PRIMARY KEY(username, pattern)
        )";

    pub const MODERATORS_TABLE_POSTGRES: &str = 
        "CREATE TABLE IF NOT EXISTS moderators (
            username TEXT REFERENCES users(username),
            pattern TEXT,
            PRIMARY KEY(username, pattern)
        )";
}