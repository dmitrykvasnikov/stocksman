CREATE TABLE user_configuration (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    document TEXT NOT NULL CHECK (json_valid(document)),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;
