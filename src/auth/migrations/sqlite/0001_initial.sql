-- Initial auth schema for SQLite

CREATE TABLE IF NOT EXISTS users (
    username TEXT PRIMARY KEY,
    password_hash TEXT NOT NULL,
    key TEXT
);

CREATE TABLE IF NOT EXISTS admins (
    username TEXT PRIMARY KEY REFERENCES users(username)
);

CREATE TABLE IF NOT EXISTS moderators (
    username TEXT REFERENCES users(username),
    pattern TEXT,
    PRIMARY KEY(username, pattern)
);

CREATE TABLE IF NOT EXISTS user_limits (
    username TEXT PRIMARY KEY REFERENCES users(username) ON DELETE CASCADE,
    can_post INTEGER,
    max_connections INTEGER,
    bandwidth_limit_bytes INTEGER,
    bandwidth_period_secs INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS user_usage (
    username TEXT PRIMARY KEY REFERENCES users(username) ON DELETE CASCADE,
    bytes_uploaded INTEGER NOT NULL DEFAULT 0,
    bytes_downloaded INTEGER NOT NULL DEFAULT 0,
    window_start_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
