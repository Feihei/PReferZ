use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use preferz_core::item::{Item, ItemId, ItemKind};
use preferz_core::scene::Scene;
use preferz_core::transform::Transform;

use crate::schema::*;

/// 视口持久化元数据（仅 `.prz` 格式存入 `metadata` 表）。
#[derive(Debug, Clone, Copy)]
pub struct ViewportMeta {
    pub pan_x: f32,
    pub pan_y: f32,
    pub zoom: f32,
}

#[derive(Debug)]
pub struct BeeFile {
    pub path: PathBuf,
    pub connection: Connection,
    pub is_prz: bool,
}

/// 加载结果：场景 + 图片字节映射（texture_id 字符串 → 原始图片字节）+ 视口元数据（仅 .prz）。
pub type LoadResult = (Scene, HashMap<String, Vec<u8>>, Option<ViewportMeta>);

impl BeeFile {
    /// 打开已存在的 .bee/.prz 文件。若 metadata 表存在且 format='prz' 则视为 .prz。
    pub fn open(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::open(path)?;
        // 检测是否为 .prz（metadata 表存在且 format='prz'）
        let is_prz: bool = conn
            .query_row(
                "SELECT value FROM metadata WHERE key = 'format'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .map(|v| v == "prz")
            .unwrap_or(false);

        Ok(Self {
            path: path.to_path_buf(),
            connection: conn,
            is_prz,
        })
    }

    /// 创建新文件并初始化 schema。按扩展名决定 .bee / .prz。
    pub fn create(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::open(path)?;
        let is_prz = path.extension().is_some_and(|e| e == "prz");
        let schema = if is_prz {
            Self::prz_schema()
        } else {
            Self::bee_schema()
        };
        conn.execute_batch(schema)?;
        Ok(Self {
            path: path.to_path_buf(),
            connection: conn,
            is_prz,
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
        CREATE TABLE IF NOT EXISTS sqlar (
            name TEXT PRIMARY KEY,
            sz INTEGER NOT NULL,
            data BLOB NOT NULL
        );
        CREATE TABLE IF NOT EXISTS metadata (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        INSERT OR REPLACE INTO metadata (key, value) VALUES ('format', 'prz');
        INSERT OR REPLACE INTO metadata (key, value) VALUES ('version', '3');
        "#
    }

    /// 保存场景。
    ///
    /// - `images`: `texture_id` 字符串 → 原始图片字节（Pixmap item 的图片数据，存入 sqlar 表）。
    ///   若某 Pixmap 的 texture_id 不在 images 中，sqlar 中对应条目保留不变（不删除）。
    /// - `viewport`: 仅 `.prz` 格式写入 metadata；`.bee` 忽略。
    ///
    /// 策略：事务内全量替换 items + 增量同步 sqlar（仅写入 images 中提供的条目），
    /// 删除场景中已不存在的 Pixmap texture_id 对应的 sqlar 条目，最后 VACUUM。
    pub fn save_scene(
        &mut self,
        scene: &Scene,
        images: &HashMap<String, Vec<u8>>,
        viewport: Option<ViewportMeta>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let tx = self.connection.transaction()?;

        // 1) 全量替换 items：先清空再插入（MVP 简化实现，配合 VACUUM 收缩空间）
        tx.execute("DELETE FROM items", [])?;

        {
            let mut stmt = tx.prepare(insert_item_query())?;
            for item in &scene.items {
                let data = serde_json::to_vec(&item.kind)?;
                let transform = serde_json::to_string(&item.transform)?;
                stmt.execute(params![
                    item.id.to_string(),
                    item_kind_str(&item.kind),
                    data,
                    transform,
                    item.z
                ])?;
            }
        }

        // 2) 收集场景中所有 Pixmap 的 texture_id（字符串），用于清理孤儿 sqlar 条目
        let mut live_textures: Vec<String> = Vec::new();
        for item in &scene.items {
            if let ItemKind::Pixmap { texture_id, .. } = &item.kind {
                live_textures.push(texture_id.to_string());
            }
        }

        // 3) 写入 images 中提供的图片字节（INSERT OR REPLACE）
        {
            let mut stmt =
                tx.prepare("INSERT OR REPLACE INTO sqlar (name, sz, data) VALUES (?, ?, ?)")?;
            for (name, bytes) in images {
                stmt.execute(params![name, bytes.len() as i64, bytes])?;
            }
        }

        // 4) 删除孤儿 sqlar 条目（场景中已不存在的 texture_id）
        if !live_textures.is_empty() {
            // 逐个删除（避免动态拼接 IN 子句的 SQL 注入风险）
            let mut stmt = tx.prepare("DELETE FROM sqlar WHERE name = ?")?;
            // 先收集所有 sqlar name，再筛除 live 的
            let all_names: Vec<String> = {
                let mut s = tx.prepare("SELECT name FROM sqlar")?;
                let rows = s.query_map([], |r| r.get::<_, String>(0))?;
                rows.filter_map(|r| r.ok()).collect()
            };
            for name in all_names {
                if !live_textures.contains(&name) {
                    stmt.execute(params![name])?;
                }
            }
        } else {
            tx.execute("DELETE FROM sqlar", [])?;
        }

        // 5) 视口元数据（仅 .prz）
        if self.is_prz {
            if let Some(v) = viewport {
                tx.execute(
                    insert_metadata_query(),
                    params!["viewport_pan_x", v.pan_x.to_string()],
                )?;
                tx.execute(
                    insert_metadata_query(),
                    params!["viewport_pan_y", v.pan_y.to_string()],
                )?;
                tx.execute(
                    insert_metadata_query(),
                    params!["viewport_zoom", v.zoom.to_string()],
                )?;
            }
            tx.execute(
                insert_metadata_query(),
                params!["next_z", scene.next_z.to_string()],
            )?;
        }

        tx.commit()?;

        // 6) VACUUM 收缩空间（事务外执行）
        self.connection.execute("VACUUM", [])?;
        Ok(())
    }

    /// 加载场景。
    ///
    /// 返回 `(Scene, images, Option<ViewportMeta>)`：
    /// - `images`: `texture_id` 字符串 → 原始图片字节（从 sqlar 表读出）
    /// - `ViewportMeta`: 仅 `.prz` 格式且 metadata 中存在视口数据时返回
    ///
    /// 加载后会清空 Text item 的 `measured_size` 与 `editing`（运行时状态不持久化）。
    pub fn load_scene(&self) -> Result<LoadResult, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut images: HashMap<String, Vec<u8>> = HashMap::new();

        // 1) 读取 items
        {
            let mut stmt = self.connection.prepare(select_all_items_query())?;
            let rows = stmt.query_map([], |row| {
                let id_str: String = row.get(0)?;
                let kind_str: String = row.get(1)?;
                let data_blob: Vec<u8> = row.get(2)?;
                let transform_str: String = row.get(3)?;
                let z: i32 = row.get(4)?;
                Ok((id_str, kind_str, data_blob, transform_str, z))
            })?;

            for row in rows {
                let (id_str, _kind_str, data_blob, transform_str, z) = row?;
                let id = ItemId::parse_str(&id_str)
                    .map_err(|e| format!("invalid item id '{}': {}", id_str, e))?;

                // 反序列化 ItemKind，清空运行时字段
                let mut kind: ItemKind = serde_json::from_slice(&data_blob)?;
                if let ItemKind::Text {
                    editing,
                    measured_size,
                    ..
                } = &mut kind
                {
                    *editing = false;
                    *measured_size = None;
                }

                let transform: Transform = serde_json::from_str(&transform_str)?;

                let item = Item {
                    id,
                    kind,
                    transform,
                    z,
                };
                // 保留 z（add_item_preserve_z 会推进 next_z）
                scene.add_item_preserve_z(item);
            }
        }

        // 2) 读取 sqlar 图片数据
        {
            let mut stmt = self.connection.prepare("SELECT name, data FROM sqlar")?;
            let rows = stmt.query_map([], |row| {
                let name: String = row.get(0)?;
                let data: Vec<u8> = row.get(1)?;
                Ok((name, data))
            })?;
            for row in rows {
                let (name, data) = row?;
                images.insert(name, data);
            }
        }

        // 3) 读取视口元数据（仅 .prz）
        let viewport = if self.is_prz {
            self.load_viewport_meta()?
        } else {
            None
        };

        Ok((scene, images, viewport))
    }

    fn load_viewport_meta(&self) -> Result<Option<ViewportMeta>, Box<dyn std::error::Error>> {
        let mut map: HashMap<String, String> = HashMap::new();
        let mut stmt = self.connection.prepare(select_metadata_query())?;
        let rows = stmt.query_map([], |row| {
            let k: String = row.get(0)?;
            let v: String = row.get(1)?;
            Ok((k, v))
        })?;
        for row in rows {
            let (k, v) = row?;
            map.insert(k, v);
        }

        match (
            map.get("viewport_pan_x"),
            map.get("viewport_pan_y"),
            map.get("viewport_zoom"),
        ) {
            (Some(x), Some(y), Some(z)) => {
                let pan_x: f32 = x.parse().unwrap_or(0.0);
                let pan_y: f32 = y.parse().unwrap_or(0.0);
                let zoom: f32 = z.parse().unwrap_or(1.0);
                Ok(Some(ViewportMeta { pan_x, pan_y, zoom }))
            }
            _ => Ok(None),
        }
    }

    pub fn close(&self) -> Result<(), Box<dyn std::error::Error>> {
        // SQLite connection 在 Drop 时自动关闭
        Ok(())
    }
}

fn item_kind_str(kind: &ItemKind) -> &'static str {
    match kind {
        ItemKind::Pixmap { .. } => "pixmap",
        ItemKind::Text { .. } => "text",
    }
}

/// 文本数据（兼容旧 schema 的序列化结构，保留供未来迁移使用）。
#[derive(Debug, Serialize, Deserialize)]
pub struct TextData {
    pub content: String,
    pub font_size: f32,
    pub color: [u8; 4],
}

#[cfg(test)]
mod tests {
    use super::*;
    use preferz_core::item::{Item, ItemKind};
    use preferz_core::spaces::CanvasVector;
    use std::path::PathBuf;

    /// 生成唯一临时文件路径（不引入 tempfile 依赖）。
    fn tmp_path(suffix: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "preferz_test_{}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            suffix
        ));
        p
    }

    #[test]
    fn prz_save_load_roundtrip() {
        let path = tmp_path("roundtrip.prz");
        // 构造场景：一个 Pixmap + 一个 Text
        let mut scene = Scene::new();
        let tex_id = 42u64;
        scene.add_item(Item::new_pixmap(
            tex_id,
            Some("test.png".to_string()),
            (100, 80),
            10.0,
            20.0,
            1.5,
            1.5,
        ));
        scene.add_item(Item::new_text(
            "你好".to_string(),
            5.0,
            6.0,
            24.0,
            [255, 255, 255, 255],
        ));
        // 给 text item 设置 measured_size（加载后应被清空）
        if let Some(item) = scene.items.last_mut() {
            if let ItemKind::Text { measured_size, .. } = &mut item.kind {
                *measured_size = Some((99.0, 99.0));
            }
        }

        let mut images = HashMap::new();
        images.insert(tex_id.to_string(), vec![1u8, 2, 3, 4, 5]);
        let viewport = ViewportMeta {
            pan_x: 100.0,
            pan_y: 200.0,
            zoom: 1.5,
        };

        // 保存
        {
            let mut bee = BeeFile::create(&path).unwrap();
            bee.save_scene(&scene, &images, Some(viewport)).unwrap();
        }

        // 加载
        let bee = BeeFile::open(&path).unwrap();
        assert!(bee.is_prz);
        let (loaded_scene, loaded_images, loaded_vp) = bee.load_scene().unwrap();

        // 验证 items 数量
        assert_eq!(loaded_scene.items.len(), 2);
        // 验证 Pixmap
        let pixmap = loaded_scene
            .items
            .iter()
            .find(|i| matches!(i.kind, ItemKind::Pixmap { .. }))
            .unwrap();
        if let ItemKind::Pixmap {
            texture_id,
            filename,
            original_size,
            ..
        } = &pixmap.kind
        {
            assert_eq!(*texture_id, tex_id);
            assert_eq!(filename.as_deref(), Some("test.png"));
            assert_eq!(*original_size, (100, 80));
        }
        assert_eq!(pixmap.transform.pos, CanvasVector::new(10.0, 20.0));
        assert_eq!(pixmap.transform.scale, CanvasVector::new(1.5, 1.5));
        // 验证 Text + measured_size 被清空
        let text = loaded_scene
            .items
            .iter()
            .find(|i| matches!(i.kind, ItemKind::Text { .. }))
            .unwrap();
        if let ItemKind::Text {
            content,
            font_size,
            measured_size,
            editing,
            ..
        } = &text.kind
        {
            assert_eq!(content, "你好");
            assert_eq_float(*font_size, 24.0);
            assert!(measured_size.is_none(), "measured_size 应被清空");
            assert!(!*editing, "editing 应为 false");
        }
        // 验证图片字节
        assert_eq!(
            loaded_images.get(&tex_id.to_string()),
            Some(&vec![1u8, 2, 3, 4, 5])
        );
        // 验证视口元数据
        let vp = loaded_vp.expect("应有视口元数据");
        assert_eq_float(vp.pan_x, 100.0);
        assert_eq_float(vp.pan_y, 200.0);
        assert_eq_float(vp.zoom, 1.5);
        // 验证 next_z
        assert_eq!(loaded_scene.next_z, scene.next_z);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn bee_format_no_metadata() {
        let path = tmp_path("noprz.bee");
        let scene = Scene::new();
        {
            let mut bee = BeeFile::create(&path).unwrap();
            bee.save_scene(&scene, &HashMap::new(), None).unwrap();
        }
        let bee = BeeFile::open(&path).unwrap();
        assert!(!bee.is_prz);
        let (_scene, _images, vp) = bee.load_scene().unwrap();
        assert!(vp.is_none(), ".bee 不应有视口元数据");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn orphan_sqlar_cleaned_on_save() {
        let path = tmp_path("orphan.prz");
        let tex_id = 7u64;
        let mut scene = Scene::new();
        scene.add_item(Item::new_pixmap(tex_id, None, (10, 10), 0.0, 0.0, 1.0, 1.0));
        let mut images = HashMap::new();
        images.insert(tex_id.to_string(), vec![9u8, 9]);

        // 第一次保存
        {
            let mut bee = BeeFile::create(&path).unwrap();
            bee.save_scene(&scene, &images, None).unwrap();
        }
        // 第二次保存：删除 Pixmap，只留 Text（sqlar 应被清空）
        let mut scene2 = Scene::new();
        scene2.add_item(Item::new_text(
            "x".to_string(),
            0.0,
            0.0,
            16.0,
            [255, 255, 255, 255],
        ));
        {
            let mut bee = BeeFile::open(&path).unwrap();
            bee.save_scene(&scene2, &HashMap::new(), None).unwrap();
        }
        let bee = BeeFile::open(&path).unwrap();
        let (_s, images2, _vp) = bee.load_scene().unwrap();
        assert!(images2.is_empty(), "孤儿 sqlar 条目应被清除");
        let _ = std::fs::remove_file(&path);
    }

    fn assert_eq_float(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-5, "float mismatch: {} vs {}", a, b);
    }
}
