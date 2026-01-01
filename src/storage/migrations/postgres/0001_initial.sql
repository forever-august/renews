-- Initial storage schema for PostgreSQL

CREATE TABLE IF NOT EXISTS messages (
    message_id TEXT PRIMARY KEY,
    headers TEXT,
    body TEXT,
    size BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS group_articles (
    group_name TEXT,
    number BIGINT,
    message_id TEXT,
    inserted_at BIGINT NOT NULL,
    PRIMARY KEY(group_name, number),
    FOREIGN KEY(message_id) REFERENCES messages(message_id)
);

CREATE TABLE IF NOT EXISTS groups (
    name TEXT PRIMARY KEY,
    created_at BIGINT NOT NULL,
    moderated BOOLEAN NOT NULL DEFAULT FALSE,
    description TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS overview (
    group_name TEXT,
    article_number BIGINT,
    overview_data TEXT,
    PRIMARY KEY(group_name, article_number)
);
