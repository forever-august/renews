-- Initial storage schema for SQLite

CREATE TABLE IF NOT EXISTS messages (
    message_id TEXT PRIMARY KEY,
    headers TEXT,
    body TEXT,
    size INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS group_articles (
    group_name TEXT,
    number INTEGER,
    message_id TEXT,
    inserted_at INTEGER NOT NULL,
    PRIMARY KEY(group_name, number),
    FOREIGN KEY(message_id) REFERENCES messages(message_id)
);

CREATE TABLE IF NOT EXISTS groups (
    name TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL,
    moderated INTEGER NOT NULL DEFAULT 0,
    description TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS overview (
    group_name TEXT,
    article_number INTEGER,
    overview_data TEXT,
    PRIMARY KEY(group_name, article_number)
);
