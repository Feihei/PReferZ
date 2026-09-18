use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use preferz_core::item::{Item, ItemId, ItemKind};
use preferz_core::scene::Scene;
use preferz_core::transform::Transform;

use crate::schema::*;

/// 视口持久化元数据（存入 `.prz` 的 `metadata` 表）。
#[derive(Debug, Clone, Copy)]
pub struct ViewportMeta {
    pub pan_x: f32,
    pub pan_y: f32,
    pub zoom: f32,
}

impl Default for ViewportMeta {
    /// 文件未记录视口时的兜底值：原点、100% 缩放。
    fn default() -> Self {
        Self {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
        }
    }
}

#[derive(Debug)]
pub struct PrzFile {
    pub path: PathBuf,
    pub connection: Connection,
}

/// 加载结果：场景 + 图片字节映射（texture_id 字符串 → 原始图片字节）+ 视口元数据。
pub type LoadResult = (Scene, HashMap<String, Vec<u8>>, ViewportMeta);

impl PrzFile {
    /// 打开已存在的 `.prz` 文件。
    ///
    /// 会校验 `metadata.format == 'prz'`；格式不符（含 BeeRef 的 `.bee`——其
    /// items 表为 9 列 INTEGER id，与本格式不兼容）直接返回错误，而不是在后续
    /// 查询里抛出难懂的 `no such column`。
    pub fn open(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::open(path)?;
        let format: Option<String> = conn
            .query_row(
                "SELECT value FROM metadata WHERE key = 'format'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok();
        match format.as_deref() {
            Some("prz") => {}
            other => {
                return Err(format!(
                    "{} 不是有效的 PReferZ 项目文件（metadata.format = {:?}，期望 \"prz\"）",
                    path.display(),
                    other.unwrap_or("<缺失>")
                )
                .into());
            }
        }

        // plan #13：旧文件（version 3 之前建的 items 表）缺 group_id 列，就地补列。
        // CREATE TABLE IF NOT EXISTS 不会给已存在的表加列，open 时迁移一次即可。
        let has_group_id: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('items') WHERE name = 'group_id'",
            [],
            |row| row.get(0),
        )?;
        if has_group_id == 0 {
            conn.execute("ALTER TABLE items ADD COLUMN group_id TEXT", [])?;
        }

        Ok(Self {
            path: path.to_path_buf(),
            connection: conn,
        })
    }

    /// 创建新文件并初始化 schema。
    pub fn create(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::open(path)?;
        conn.execute_batch(Self::schema())?;
        Ok(Self {
            path: path.to_path_buf(),
            connection: conn,
        })
    }

    fn schema() -> &'static str {
        r#"
        CREATE TABLE IF NOT EXISTS items (
            id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            data BLOB NOT NULL,
            transform TEXT NOT NULL,
            z INTEGER NOT NULL,
            group_id TEXT
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
    /// - `viewport`: 视口状态，写入 metadata 表。
    ///
    /// 策略：事务内全量替换 items + 增量同步 sqlar（仅写入 images 中提供的条目），
    /// 删除场景中已不存在的 Pixmap texture_id 对应的 sqlar 条目，最后 VACUUM。
    pub fn save_scene(
        &mut self,
        scene: &Scene,
        images: &HashMap<String, Vec<u8>>,
        viewport: ViewportMeta,
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
                    item.z,
                    item.group_id.map(|g| g.to_string()),
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

        // 5) 视口元数据
        tx.execute(
            insert_metadata_query(),
            params!["viewport_pan_x", viewport.pan_x.to_string()],
        )?;
        tx.execute(
            insert_metadata_query(),
            params!["viewport_pan_y", viewport.pan_y.to_string()],
        )?;
        tx.execute(
            insert_metadata_query(),
            params!["viewport_zoom", viewport.zoom.to_string()],
        )?;
        tx.execute(
            insert_metadata_query(),
            params!["next_z", scene.next_z.to_string()],
        )?;

        tx.commit()?;

        // 6) VACUUM 收缩空间（事务外执行）
        self.connection.execute("VACUUM", [])?;
        Ok(())
    }

    /// 加载场景。
    ///
    /// 返回 `(Scene, images, ViewportMeta)`：
    /// - `images`: `texture_id` 字符串 → 原始图片字节（从 sqlar 表读出）
    /// - `ViewportMeta`: metadata 中的视口数据；文件未记录时返回 [`ViewportMeta::default`]
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
                let group_id: Option<String> = row.get(5)?;
                Ok((id_str, kind_str, data_blob, transform_str, z, group_id))
            })?;

            for row in rows {
                let (id_str, _kind_str, data_blob, transform_str, z, group_id_str) = row?;
                let id = ItemId::parse_str(&id_str)
                    .map_err(|e| format!("invalid item id '{}': {}", id_str, e))?;
                let group_id = match group_id_str {
                    Some(s) => Some(
                        ItemId::parse_str(&s)
                            .map_err(|e| format!("invalid group id '{}': {}", s, e))?,
                    ),
                    None => None,
                };

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
                    group_id,
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

        // 3) 读取视口元数据
        let viewport = self.load_viewport_meta()?;

        Ok((scene, images, viewport))
    }

    fn load_viewport_meta(&self) -> Result<ViewportMeta, Box<dyn std::error::Error>> {
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
            (Some(x), Some(y), Some(z)) => Ok(ViewportMeta {
                pan_x: x.parse().unwrap_or_default(),
                pan_y: y.parse().unwrap_or_default(),
                zoom: z.parse().unwrap_or(1.0),
            }),
            _ => Ok(ViewportMeta::default()),
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
        ItemKind::Shape { .. } => "shape",
        ItemKind::Frame { .. } => "frame",
        ItemKind::Freedraw { .. } => "freedraw",
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
    use preferz_core::shape::{ArrowHeadStyle, ShapeType, StrokeStyle};
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
            let mut prz = PrzFile::create(&path).unwrap();
            prz.save_scene(&scene, &images, viewport).unwrap();
        }

        // 加载
        let prz = PrzFile::open(&path).unwrap();
        let (loaded_scene, loaded_images, loaded_vp) = prz.load_scene().unwrap();

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
        assert_eq_float(loaded_vp.pan_x, 100.0);
        assert_eq_float(loaded_vp.pan_y, 200.0);
        assert_eq_float(loaded_vp.zoom, 1.5);
        // 验证 next_z
        assert_eq!(loaded_scene.next_z, scene.next_z);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn prz_save_load_line_roundtrip() {
        let path = tmp_path("line_roundtrip.prz");
        // 构造场景：一个线性对象 Polyline（终点箭头）
        let mut scene = Scene::new();
        scene.add_item(Item::new_polyline(
            vec![(0.0, 0.0), (80.0, 40.0)],
            (80.0, 40.0),
            None,
            Some(ArrowHeadStyle::Arrow),
            false,
            30.0,
            40.0,
            StrokeStyle::default(),
        ));

        let viewport = ViewportMeta {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
        };
        {
            let mut prz = PrzFile::create(&path).unwrap();
            prz.save_scene(&scene, &HashMap::new(), viewport).unwrap();
        }

        // 加载
        let prz = PrzFile::open(&path).unwrap();
        let (loaded_scene, _, _) = prz.load_scene().unwrap();
        assert_eq!(loaded_scene.items.len(), 1);
        let line = &loaded_scene.items[0];
        match &line.kind {
            ItemKind::Shape {
                shape_type,
                base_size,
                points,
                start_arrow,
                end_arrow,
                closed,
                ..
            } => {
                assert_eq!(*shape_type, ShapeType::Polyline);
                assert_eq!(*base_size, (80.0, 40.0));
                assert_eq!(points, &vec![(0.0, 0.0), (80.0, 40.0)]);
                assert_eq!(*start_arrow, None);
                assert_eq!(*end_arrow, Some(ArrowHeadStyle::Arrow));
                assert!(!*closed);
            }
            _ => panic!("expected Shape kind"),
        }
        assert_eq!(line.transform.pos, CanvasVector::new(30.0, 40.0));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn prz_save_load_freedraw_roundtrip() {
        // plan #10：墨迹 kind 存于 data JSON blob，无 schema 迁移——存读保真。
        let path = tmp_path("freedraw_roundtrip.prz");
        let mut scene = Scene::new();
        scene.add_item(Item::new_freedraw(
            &[(50.0, 60.0), (90.0, 60.0), (110.0, 100.0)],
            &[5.0, 3.0, 1.5],
            [200, 40, 40, 255],
        ));
        let viewport = ViewportMeta {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
        };
        {
            let mut prz = PrzFile::create(&path).unwrap();
            prz.save_scene(&scene, &HashMap::new(), viewport).unwrap();
        }
        let prz = PrzFile::open(&path).unwrap();
        let (loaded, _, _) = prz.load_scene().unwrap();
        assert_eq!(loaded.items.len(), 1);
        let it = &loaded.items[0];
        // 中心线 AABB 左上角 (50,60) 落进 transform.pos，点归一到局部。
        assert_eq!(it.transform.pos, CanvasVector::new(50.0, 60.0));
        match &it.kind {
            ItemKind::Freedraw {
                points,
                widths,
                color,
            } => {
                assert_eq!(points, &vec![(0.0, 0.0), (40.0, 0.0), (60.0, 40.0)]);
                assert_eq!(widths, &vec![5.0, 3.0, 1.5]);
                assert_eq!(*color, [200, 40, 40, 255]);
            }
            _ => panic!("expected Freedraw kind"),
        }
        let _ = std::fs::remove_file(&path);
    }

    /// 非 PReferZ 的 SQLite 文件（含 BeeRef 的 .bee）应被 `open` 明确拒绝，
    /// 而不是在后续查询里抛出 `no such column` 之类的底层错误。
    #[test]
    fn open_rejects_non_prz_file() {
        let path = tmp_path("foreign.bee");
        {
            // 模拟 BeeRef 的 items 表：9 列、INTEGER id、transform 分列存储
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE items (
                    id INTEGER PRIMARY KEY,
                    type TEXT NOT NULL,
                    x REAL DEFAULT 0,
                    y REAL DEFAULT 0,
                    z REAL DEFAULT 0,
                    scale REAL DEFAULT 1,
                    rotation REAL DEFAULT 0,
                    flip INTEGER DEFAULT 1,
                    data JSON
                );
                CREATE TABLE sqlar (
                    name TEXT PRIMARY KEY,
                    item_id INTEGER NOT NULL UNIQUE,
                    mode INT,
                    mtime INT,
                    sz INT,
                    data BLOB
                );",
            )
            .unwrap();
        }
        let err = PrzFile::open(&path).expect_err("应拒绝非 prz 文件");
        assert!(
            err.to_string().contains("不是有效的 PReferZ 项目文件"),
            "错误信息应可读，实际: {err}"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// 文件未记录视口时，`load_scene` 应返回默认视口而非失败。
    #[test]
    fn load_scene_falls_back_to_default_viewport() {
        let path = tmp_path("noviewport.prz");
        {
            let mut prz = PrzFile::create(&path).unwrap();
            prz.save_scene(&Scene::new(), &HashMap::new(), ViewportMeta::default())
                .unwrap();
            // 清掉视口元数据，模拟早期文件
            prz.connection
                .execute("DELETE FROM metadata WHERE key LIKE 'viewport%'", [])
                .unwrap();
        }
        let prz = PrzFile::open(&path).unwrap();
        let (_scene, _images, vp) = prz.load_scene().unwrap();
        assert_eq_float(vp.pan_x, 0.0);
        assert_eq_float(vp.pan_y, 0.0);
        assert_eq_float(vp.zoom, 1.0);
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
            let mut prz = PrzFile::create(&path).unwrap();
            prz.save_scene(&scene, &images, ViewportMeta::default())
                .unwrap();
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
            let mut prz = PrzFile::open(&path).unwrap();
            prz.save_scene(&scene2, &HashMap::new(), ViewportMeta::default())
                .unwrap();
        }
        let prz = PrzFile::open(&path).unwrap();
        let (_s, images2, _vp) = prz.load_scene().unwrap();
        assert!(images2.is_empty(), "孤儿 sqlar 条目应被清除");
        let _ = std::fs::remove_file(&path);
    }

    fn assert_eq_float(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-5, "float mismatch: {} vs {}", a, b);
    }

    // ─────────────────────── 编组持久化（plan #13） ───────────────────────

    #[test]
    fn prz_save_load_group_roundtrip() {
        let path = tmp_path("group.prz");
        let mut scene = Scene::new();
        let a = Item::new_shape(
            ShapeType::Rectangle,
            (50.0, 50.0),
            0.0,
            0.0,
            StrokeStyle::default(),
            None,
        );
        let b = Item::new_shape(
            ShapeType::Rectangle,
            (50.0, 50.0),
            100.0,
            0.0,
            StrokeStyle::default(),
            None,
        );
        let (a_id, b_id) = (a.id, b.id);
        scene.add_item(a);
        scene.add_item(b);
        let gid = scene.group(&[a_id, b_id]).unwrap();

        {
            let mut prz = PrzFile::create(&path).unwrap();
            prz.save_scene(&scene, &HashMap::new(), ViewportMeta::default())
                .unwrap();
        }
        let prz = PrzFile::open(&path).unwrap();
        let (loaded, _images, _vp) = prz.load_scene().unwrap();
        assert_eq!(loaded.items.len(), 2);
        assert_eq!(loaded.get_item(&a_id).unwrap().group_id, Some(gid));
        assert_eq!(loaded.get_item(&b_id).unwrap().group_id, Some(gid));
        let _ = std::fs::remove_file(&path);
    }

    /// 旧版 .prz（items 表 5 列、无 group_id 列）：open 应自动补列，加载不失败。
    #[test]
    fn prz_open_migrates_legacy_items_without_group_column() {
        use rusqlite::Connection;
        let path = tmp_path("legacy5col.prz");
        // ItemId = uuid::Uuid 别名；fileio 不直接依赖 uuid crate，经 core 间接使用
        let a_id = preferz_core::item::ItemId::new_v4();
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE items (
                    id TEXT PRIMARY KEY,
                    kind TEXT NOT NULL,
                    data BLOB NOT NULL,
                    transform TEXT NOT NULL,
                    z INTEGER NOT NULL
                );
                CREATE TABLE sqlar (name TEXT PRIMARY KEY, sz INTEGER NOT NULL, data BLOB NOT NULL);
                CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                INSERT INTO metadata (key, value) VALUES ('format', 'prz');",
            )
            .unwrap();
            let shape_kind = serde_json::json!({
                "Shape": {
                    "shape_type": "Rectangle",
                    "base_size": [50.0, 50.0],
                    "points": [],
                    "stroke": { "color": [30,30,30,255], "width": 2.0, "dash": "Solid" },
                    "fill": null,
                    "start_arrow": null,
                    "end_arrow": null,
                    "seed": 0
                }
            });
            conn.execute(
                "INSERT INTO items (id, kind, data, transform, z) VALUES (?, ?, ?, ?, ?)",
                rusqlite::params![
                    a_id.to_string(),
                    "shape",
                    serde_json::to_vec(&shape_kind).unwrap(),
                    serde_json::to_string(&serde_json::json!({
                        "pos": [0.0, 0.0],
                        "scale": [1.0, 1.0],
                        "rotation": 0.0,
                        "flip_h": false,
                        "flip_v": false
                    }))
                    .unwrap(),
                    0
                ],
            )
            .unwrap();
        }

        let prz = PrzFile::open(&path).unwrap();
        let (scene, _images, _vp) = prz.load_scene().unwrap();
        assert_eq!(scene.items.len(), 1);
        assert_eq!(scene.items[0].id, a_id);
        assert_eq!(scene.items[0].group_id, None, "旧文件加载后应未编组");
        let _ = std::fs::remove_file(&path);
    }
}
