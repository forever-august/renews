-- Initial auth schema for PostgreSQL

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
    can_post BOOLEAN,
    max_connections INTEGER,
    bandwidth_limit_bytes BIGINT,
    bandwidth_period_secs BIGINT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS user_usage (
    username TEXT PRIMARY KEY REFERENCES users(username) ON DELETE CASCADE,
    bytes_uploaded BIGINT NOT NULL DEFAULT 0,
    bytes_downloaded BIGINT NOT NULL DEFAULT 0,
    window_start_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);
