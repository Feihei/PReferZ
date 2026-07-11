use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use preferz_core::Scene;

use crate::schema::*;

#[derive(Debug)]
pub struct BeeFile {
    pub path: PathBuf,
    pub connection: Connection,
    pub is_prz: bool,
}

impl BeeFile {
    pub fn open(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::open(path)?;
        let format: Option<String> = conn.query_row("SELECT value FROM metadata WHERE key = 'format'", [], |row| {
            row.get::<_, String>(0)
        }).ok();
        let is_prz = format.as_deref() == Some("prz");

        Ok(Self {
            path: path.to_path_buf(),
            connection: conn,
            is_prz,
        })
    }

    pub fn create(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::open(path)?;
        let schema = if path.extension().map_or(false, |e| e == "prz") {
            // .prz format with metadata
            Self::prz_schema()
        } else {
            // .bee format without metadata
            Self::bee_schema()
        };

        conn.execute_batch(&schema)?;
        Ok(Self {
            path: path.to_path_buf(),
            connection: conn,
            is_prz: path.extension().map_or(false, |e| e == "prz"),
        })
    }

    fn bee_schema() -> &'static str {
        r#"
        CREATE TABLE IF NOT EXISTS items (
            id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            data BLOB NOT NULL,
            transform TEXT NOT NULL,
            z INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS sqlar (
            name TEXT PRIMARY KEY,
            sz INTEGER NOT NULL,
            data BLOB NOT NULL
        );
        "#
    }

    fn prz_schema() -> &'static str {
        r#"
        CREATE TABLE IF NOT EXISTS items (
            id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            data BLOB NOT NULL,
            transform TEXT NOT NULL,
            z INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS metadata (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        INSERT OR REPLACE INTO metadata (key, value) VALUES ('format', 'prz');
        INSERT OR REPLACE INTO metadata (key, value) VALUES ('version', '3');
        "#
    }

    pub fn save_scene(&self, scene: &Scene) -> Result<(), Box<dyn std::error::Error>> {
        let mut stmt = self.connection.prepare(insert_item_query())?;

        for item in &scene.items {
            let data = serde_json::to_vec(&item.kind)?;
            let transform = serde_json::to_vec(&item.transform)?;

            stmt.execute(params![
                item.id.to_string(),
                self.item_kind_str(&item.kind),
                data,
                transform,
                item.z
            ])?;
        }

        Ok(())
    }

    fn item_kind_str(&self, kind: &preferz_core::ItemKind) -> String {
        match kind {
            preferz_core::ItemKind::Pixmap { .. } => "pixmap".to_string(),
            preferz_core::ItemKind::Text { .. } => "text".to_string(),
        }
    }

    pub fn load_scene(&self) -> Result<Scene, Box<dyn std::error::Error>> {
        // 简化实现：返回空场景
        Ok(Scene::new())
    }

    pub fn close(&self) -> Result<(), Box<dyn std::error::Error>> {
        // SQLite connection is dropped automatically
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TextData {
    pub content: String,
    pub font_size: f32,
    pub color: [u8; 4],
}
