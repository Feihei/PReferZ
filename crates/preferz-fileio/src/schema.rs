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
