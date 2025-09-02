CREATE TABLE received_activities (
    activity_id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL,
    object TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%FT%TZ', CURRENT_TIMESTAMP))
) STRICT;
CREATE TABLE users (
    user_id INTEGER PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL
) STRICT;
CREATE TABLE sessions (
    token TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users ON UPDATE CASCADE ON DELETE CASCADE,
    timestamp TEXT NOT NULL DEFAULT (strftime('%FT%TZ', CURRENT_TIMESTAMP))
) STRICT;
CREATE TABLE cache (
    id TEXT PRIMARY KEY,
    actor TEXT,
    type TEXT,
    object TEXT
);
CREATE TABLE IF NOT EXISTS "notifications_follows" (
    notification_id INTEGER NOT NULL REFERENCES notifications ON DELETE CASCADE ON UPDATE CASCADE,
    actor_id TEXT NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS "notifications_announcements" (
    notification_id INTEGER NOT NULL REFERENCES notifications ON DELETE CASCADE ON UPDATE CASCADE,
    note_id TEXT REFERENCES notes ON UPDATE CASCADE ON DELETE CASCADE,
    actor_id TEXT NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS "notifications_likes" (
    notification_id INTEGER NOT NULL REFERENCES notifications ON DELETE CASCADE ON UPDATE CASCADE,
    note_id TEXT REFERENCES notes ON UPDATE CASCADE ON DELETE CASCADE,
    actor_id TEXT NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS "note_likes" (
    activity_id TEXT PRIMARY KEY,
    note_id TEXT REFERENCES notes ON UPDATE CASCADE ON DELETE CASCADE,
    actor_id TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%FT%TZ', CURRENT_TIMESTAMP)),
    UNIQUE (note_id, actor_id) ON CONFLICT REPLACE
) STRICT;
CREATE TABLE IF NOT EXISTS "note_announcements" (
    activity_id TEXT PRIMARY KEY,
    note_id TEXT REFERENCES notes ON UPDATE CASCADE ON DELETE CASCADE,
    actor_id TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%FT%TZ', CURRENT_TIMESTAMP)),
    UNIQUE (note_id, actor_id) ON CONFLICT REPLACE
) STRICT;
CREATE TABLE IF NOT EXISTS "notifications_replies" (
    notification_id INTEGER NOT NULL REFERENCES notifications ON DELETE CASCADE ON UPDATE CASCADE,
    note_id TEXT,
    replied_to_note_id TEXT REFERENCES notes ON UPDATE CASCADE ON DELETE CASCADE,
    actor_id TEXT NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS "actors" (
    id TEXT PRIMARY KEY,
    uuid TEXT UNIQUE NOT NULL,
    display_name TEXT NOT NULL,
    preferred_username TEXT NOT NULL,
    nickname TEXT,
    private_key_pem TEXT NOT NULL,
    icon_url TEXT
) STRICT;
CREATE TABLE IF NOT EXISTS "sent_activities" (
    activity_id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL REFERENCES actors ON DELETE CASCADE ON UPDATE CASCADE,
    object TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%FT%TZ', CURRENT_TIMESTAMP))
) STRICT;
CREATE TABLE IF NOT EXISTS "notifications" (
    notification_id INTEGER PRIMARY KEY,
    activity_id TEXT,
    actor_id TEXT NOT NULL REFERENCES "actors" ON DELETE CASCADE ON UPDATE CASCADE,
    is_read INTEGER NOT NULL DEFAULT FALSE,
    created_at TEXT NOT NULL DEFAULT (strftime('%FT%TZ', CURRENT_TIMESTAMP))
) STRICT;
CREATE TABLE IF NOT EXISTS "followers" (
    activity_id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL REFERENCES "actors" ON DELETE CASCADE ON UPDATE CASCADE,
    sender_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%FT%TZ', CURRENT_TIMESTAMP)),
    UNIQUE (actor_id, sender_id) ON CONFLICT REPLACE
) STRICT;
CREATE TABLE IF NOT EXISTS "following" (
    actor_id TEXT NOT NULL REFERENCES "actors" ON DELETE CASCADE ON UPDATE CASCADE,
    followed_actor_id TEXT NOT NULL,
    activity_id TEXT,
    is_pending INTEGER DEFAULT TRUE,
    accept_activity TEXT,
    UNIQUE (actor_id, followed_actor_id)
) STRICT;
CREATE TABLE IF NOT EXISTS "likes" (
    activity_id TEXT PRIMARY KEY,
    actor_id TEXT REFERENCES "actors" ON UPDATE CASCADE ON DELETE CASCADE,
    note_id TEXT
) STRICT;
CREATE TABLE IF NOT EXISTS "reposts" (
    activity_id TEXT PRIMARY KEY,
    actor_id TEXT REFERENCES "actors" ON UPDATE CASCADE ON DELETE CASCADE,
    note_id TEXT
) STRICT;
CREATE TABLE IF NOT EXISTS "globals" (
    key TEXT NOT NULL UNIQUE,
    value TEXT NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS "notes" (
    id TEXT PRIMARY KEY,
    uuid TEXT NOT NULL,
    actor_id TEXT NOT NULL REFERENCES "actors" ON DELETE CASCADE ON UPDATE CASCADE,
    object TEXT NOT NULL
);
