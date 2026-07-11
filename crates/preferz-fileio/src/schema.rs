pub const USER_VERSION: u32 = 3;

// .bee 格式 (USER_VERSION = 2)
pub const BEE_SCHEMA_VERSION: u32 = 2;

// .prz 格式 (USER_VERSION = 3)
pub const PRZ_SCHEMA_VERSION: u32 = 3;

pub fn create_schema() -> &'static str {
    r#"
    -- Items table
    CREATE TABLE IF NOT EXISTS items (
        id TEXT PRIMARY KEY,
        kind TEXT NOT NULL,
        data BLOB NOT NULL,
        transform TEXT NOT NULL,
        z INTEGER NOT NULL
    );

    -- For .prz format (USER_VERSION >= 3)
    CREATE TABLE IF NOT EXISTS metadata (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );

    -- For .bee format (USER_VERSION = 2)
    CREATE TABLE IF NOT EXISTS sqlar (
        name TEXT PRIMARY KEY,
        sz INTEGER NOT NULL,
        data BLOB NOT NULL
    );
    "#
}

pub fn insert_item_query() -> &'static str {
    "INSERT OR REPLACE INTO items (id, kind, data, transform, z) VALUES (?, ?, ?, ?, ?)"
}

pub fn select_all_items_query() -> &'static str {
    "SELECT id, kind, data, transform, z FROM items ORDER BY z"
}

pub fn delete_item_query() -> &'static str {
    "DELETE FROM items WHERE id = ?"
}

pub fn insert_metadata_query() -> &'static str {
    "INSERT OR REPLACE INTO metadata (key, value) VALUES (?, ?)"
}

pub fn select_metadata_query() -> &'static str {
    "SELECT key, value FROM metadata"
}
