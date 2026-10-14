//! 请求分发：解析 `cmd` 并在 [`CliDoc`] 上执行，返回统一 JSON 响应。
//!
//! 所有写操作都经 `session.push(Command)` 走 undo 通道，与 GUI 共享同一套
//! core 命令语义；headless 调用方没有"拖拽预览直改"阶段，故 `MoveItems`
//! 等默认 preview 的命令必须显式 `with_preview_applied(false)`。

use preferz_core::commands::{AddItem, DeleteItems, MoveItems};
use preferz_core::shape::StrokeStyle;
use preferz_core::spaces::CanvasVector;
use preferz_core::{Item, ItemId, ItemKind, ShapeType, Transform};
use serde_json::{json, Value};

use crate::doc::CliDoc;
use crate::protocol::{
    self, ok, opt_bool, opt_f32, opt_str, parse_color, parse_id, req_f32, req_str,
};

/// 分发一条已解析为 JSON 值的请求。永不 panic：任何错误都收敛成失败响应。
pub(crate) fn dispatch(doc: &mut CliDoc, req: &Value) -> Value {
    let Some(cmd) = req.get("cmd").and_then(Value::as_str) else {
        return protocol::fail("<none>", req, "request must be an object with string 'cmd'");
    };
    match run(doc, cmd, req) {
        Ok(data) => ok(cmd, req, data),
        Err(msg) => protocol::fail(cmd, req, msg),
    }
}

fn run(doc: &mut CliDoc, cmd: &str, req: &Value) -> Result<Value, String> {
    match cmd {
        "ping" => Ok(json!({ "pong": true, "version": crate_version() })),
        "version" => Ok(json!({ "version": crate_version(), "format": "prz" })),

        "new" => {
            doc.reset();
            Ok(json!({ "cleared": true }))
        }
        "open" => {
            let path = req_str(req, "path")?;
            let count = doc.open(std::path::Path::new(path))?;
            Ok(json!({ "path": path, "items": count }))
        }
        "save" => {
            let target = opt_str(req, "path").map(std::path::Path::new);
            let saved = doc.save(target)?;
            Ok(json!({ "path": saved.to_string_lossy() }))
        }

        "stats" => Ok(stats(doc)),
        "list" => Ok(json!({ "items": list_items(doc) })),
        "get" => {
            let id = parse_id(req_str(req, "id")?)?;
            let item = doc
                .session
                .scene
                .get_item(&id)
                .ok_or_else(|| format!("no item with id {id}"))?;
            serde_json::to_value(item).map_err(|e| format!("serialize item failed: {e}"))
        }

        "select" => {
            let ids = protocol::opt_ids(req)?.ok_or_else(|| "missing 'ids' array".to_string())?;
            if !opt_bool(req, "additive", false) {
                doc.session.scene.deselect_all();
            }
            let mut selected = 0usize;
            for id in ids {
                if doc.session.scene.get_item(&id).is_some() {
                    doc.session.scene.select(id);
                    selected += 1;
                }
            }
            Ok(json!({ "selected": selected }))
        }
        "select_all" => {
            doc.session.scene.select_all();
            Ok(json!({ "selected": doc.session.scene.selection.len() }))
        }
        "deselect" => {
            doc.session.scene.deselect_all();
            Ok(json!({ "selected": 0 }))
        }

        "add_shape" => add_shape(doc, req).map(|id| json!({ "id": id.to_string() })),
        "add_text" => add_text(doc, req).map(|id| json!({ "id": id.to_string() })),
        "add_item" => add_item_raw(doc, req).map(|id| json!({ "id": id.to_string() })),

        "delete" => {
            let ids = target_ids(doc, req)?;
            if ids.is_empty() {
                return Err("no ids given and selection is empty".to_string());
            }
            let n = ids.len();
            doc.session.push(Box::new(DeleteItems::new(ids)));
            Ok(json!({ "deleted": n }))
        }

        "move" => {
            let ids = target_ids(doc, req)?;
            if ids.is_empty() {
                return Err("no ids given and selection is empty".to_string());
            }
            let dx = req_f32(req, "dx")?;
            let dy = req_f32(req, "dy")?;
            if !dx.is_finite() || !dy.is_finite() {
                return Err("dx/dy must be finite numbers".to_string());
            }
            let n = ids.len();
            doc.session.push(Box::new(
                MoveItems::new(ids, CanvasVector::new(dx, dy)).with_preview_applied(false),
            ));
            Ok(json!({ "moved": n, "dx": dx, "dy": dy }))
        }

        "undo" => Ok(json!({ "performed": doc.session.undo() })),
        "redo" => Ok(json!({ "performed": doc.session.redo() })),

        other => Err(format!("unknown command '{other}'")),
    }
}

fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// 命令的目标 id 列表：显式 `ids` 优先，否则回退到当前选区。
fn target_ids(doc: &CliDoc, req: &Value) -> Result<Vec<ItemId>, String> {
    match protocol::opt_ids(req)? {
        Some(ids) => Ok(ids),
        None => Ok(doc.session.scene.selection.iter().copied().collect()),
    }
}

fn stats(doc: &CliDoc) -> Value {
    let s = &doc.session;
    json!({
        "path": doc.path().map(|p| p.to_string_lossy().to_string()),
        "items": s.scene.items.len(),
        "selected": s.scene.selection.len(),
        "undo_depth": s.undo_stack.undo_len(),
        "redo_depth": s.undo_stack.redo_len(),
        "next_z": s.scene.next_z,
        "viewport": {
            "pan_x": s.viewport.pan.x,
            "pan_y": s.viewport.pan.y,
            "zoom": s.viewport.zoom,
        },
    })
}

/// list 元素的类型标签（小写，稳定的协议字面量）。
fn kind_label(kind: &ItemKind) -> &'static str {
    match kind {
        ItemKind::Pixmap { .. } => "pixmap",
        ItemKind::Text { .. } => "text",
        ItemKind::Shape { .. } => "shape",
        ItemKind::Frame { .. } => "frame",
        ItemKind::Freedraw { .. } => "freedraw",
        ItemKind::Chart { .. } => "chart",
    }
}

/// 每个 item 一行精简摘要（z 序倒序，顶层在前）。
fn list_items(doc: &CliDoc) -> Vec<Value> {
    doc.session
        .scene
        .items_by_z_order()
        .into_iter()
        .rev()
        .map(|item| {
            let mut v = json!({
                "id": item.id.to_string(),
                "kind": kind_label(&item.kind),
                "z": item.z,
                "x": item.transform.pos.x,
                "y": item.transform.pos.y,
                "selected": doc.session.scene.selection_contains(&item.id),
            });
            let map = v.as_object_mut().expect("object");
            match &item.kind {
                ItemKind::Shape {
                    shape_type,
                    base_size,
                    points,
                    closed,
                    ..
                } => {
                    map.insert("shape".to_string(), json!(format!("{shape_type:?}")));
                    map.insert("w".to_string(), json!(base_size.0));
                    map.insert("h".to_string(), json!(base_size.1));
                    map.insert("points".to_string(), json!(points.len()));
                    map.insert("closed".to_string(), json!(closed));
                }
                ItemKind::Text {
                    content,
                    font_size,
                    container_id,
                    ..
                } => {
                    let preview: String = content.chars().take(80).collect();
                    map.insert("text".to_string(), json!(preview));
                    map.insert("font_size".to_string(), json!(*font_size));
                    if let Some(cid) = container_id {
                        map.insert("container".to_string(), json!(cid.to_string()));
                    }
                }
                ItemKind::Frame {
                    base_size,
                    number,
                    name,
                    ..
                } => {
                    map.insert("number".to_string(), json!(*number));
                    map.insert("w".to_string(), json!(base_size.0));
                    map.insert("h".to_string(), json!(base_size.1));
                    if let Some(n) = name {
                        map.insert("name".to_string(), json!(n));
                    }
                }
                ItemKind::Pixmap {
                    texture_id,
                    filename,
                    original_size,
                    ..
                } => {
                    map.insert("texture_id".to_string(), json!(*texture_id));
                    if let Some(f) = filename {
                        map.insert("filename".to_string(), json!(f));
                    }
                    map.insert("w".to_string(), json!(original_size.0));
                    map.insert("h".to_string(), json!(original_size.1));
                }
                ItemKind::Freedraw {
                    points,
                    stroke_width,
                    ..
                } => {
                    map.insert("points".to_string(), json!(points.len()));
                    map.insert("stroke_width".to_string(), json!(*stroke_width));
                }
                ItemKind::Chart {
                    chart_type,
                    base_size,
                    labels,
                    ..
                } => {
                    map.insert("chart".to_string(), json!(format!("{chart_type:?}")));
                    map.insert("w".to_string(), json!(base_size.0));
                    map.insert("h".to_string(), json!(base_size.1));
                    map.insert("series".to_string(), json!(labels.len()));
                }
            }
            if let Some(gid) = item.group_id {
                map.insert("group".to_string(), json!(gid.to_string()));
            }
            if let Some(link) = &item.link {
                map.insert("link".to_string(), json!(link));
            }
            v
        })
        .collect()
}

fn add_shape(doc: &mut CliDoc, req: &Value) -> Result<ItemId, String> {
    let shape = match req_str(req, "shape")?.to_ascii_lowercase().as_str() {
        "rectangle" | "rect" => ShapeType::Rectangle,
        "ellipse" | "circle" => ShapeType::Ellipse,
        "diamond" => ShapeType::Diamond,
        other => {
            return Err(format!(
                "unknown shape '{other}' (supported: rectangle, ellipse, diamond)"
            ))
        }
    };
    let x = opt_f32(req, "x", 0.0)?;
    let y = opt_f32(req, "y", 0.0)?;
    let w = req_f32(req, "w").or_else(|_| req_f32(req, "width"))?;
    let h = req_f32(req, "h").or_else(|_| req_f32(req, "height"))?;
    if w <= 0.0 || h <= 0.0 || !w.is_finite() || !h.is_finite() {
        return Err("w/h must be positive finite numbers".to_string());
    }

    let mut stroke = StrokeStyle::default();
    if let Some(v) = req.get("stroke") {
        stroke.color = parse_color(v)?;
    }
    let stroke_width = opt_f32(req, "stroke_width", stroke.width)?;
    if stroke_width > 0.0 && stroke_width.is_finite() {
        stroke.width = stroke_width;
    } else {
        return Err("stroke_width must be a positive finite number".to_string());
    }
    let fill = match req.get("fill") {
        Some(Value::Null) | None => None,
        Some(v) => Some(parse_color(v)?),
    };

    let mut item = Item::new_shape(shape, (w, h), x, y, stroke, fill);
    // 显式给了填充色就解除"跟随描边色"，否则改描边会偷偷改填充。
    if fill.is_some() {
        if let ItemKind::Shape {
            fill_follow_stroke, ..
        } = &mut item.kind
        {
            *fill_follow_stroke = false;
        }
    }
    let id = item.id;
    doc.session.push(Box::new(AddItem::new(item)));
    Ok(id)
}

fn add_text(doc: &mut CliDoc, req: &Value) -> Result<ItemId, String> {
    let text = req_str(req, "text")?.to_string();
    let x = opt_f32(req, "x", 0.0)?;
    let y = opt_f32(req, "y", 0.0)?;
    let font_size = opt_f32(req, "font_size", 20.0)?;
    if font_size <= 0.0 {
        return Err("font_size must be positive".to_string());
    }
    let color = match req.get("color") {
        Some(v) => parse_color(v)?,
        None => [30, 30, 30, 255],
    };
    let item = Item::new_text(text, x, y, font_size, color);
    let id = item.id;
    doc.session.push(Box::new(AddItem::new(item)));
    Ok(id)
}

/// 通用转义舱：直接传 `kind`（ItemKind 的 JSON 形态，与 `.prz` 内 kind blob 同源）
/// 创建任意类型元素，可选 `x`/`y` 指定位置。
fn add_item_raw(doc: &mut CliDoc, req: &Value) -> Result<ItemId, String> {
    let kind_value = req
        .get("kind")
        .ok_or("missing 'kind' object (ItemKind JSON, same shape as in .prz files)")?
        .clone();
    let kind: ItemKind =
        serde_json::from_value(kind_value).map_err(|e| format!("invalid ItemKind: {e}"))?;
    let x = opt_f32(req, "x", 0.0)?;
    let y = opt_f32(req, "y", 0.0)?;
    let item = Item {
        id: ItemId::new_v4(),
        kind,
        transform: Transform::new(x, y, 1.0, 1.0),
        z: 0,
        group_id: None,
        link: None,
    };
    let id = item.id;
    doc.session.push(Box::new(AddItem::new(item)));
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn handle(doc: &mut CliDoc, v: Value) -> Value {
        dispatch(doc, &v)
    }

    #[test]
    fn full_headless_workflow() {
        let mut doc = CliDoc::new();

        let r = handle(&mut doc, json!({"cmd": "ping"}));
        assert!(r["ok"].as_bool().unwrap());

        // 加一个矩形
        let r = handle(
            &mut doc,
            json!({"cmd": "add_shape", "shape": "rect", "x": 10.0, "y": 20.0, "w": 100.0, "h": 50.0}),
        );
        assert!(r["ok"].as_bool().unwrap(), "{r}");
        let id = r["id"].as_str().unwrap().to_string();

        // list / stats / get
        let r = handle(&mut doc, json!({"cmd": "list"}));
        assert_eq!(r["items"].as_array().unwrap().len(), 1);
        assert_eq!(r["items"][0]["kind"], "shape");
        assert_eq!(r["items"][0]["shape"], "Rectangle");

        let r = handle(&mut doc, json!({"cmd": "stats"}));
        assert_eq!(r["items"], 1);
        assert_eq!(r["undo_depth"], 1);

        let r = handle(&mut doc, json!({"cmd": "get", "id": id}));
        assert_eq!(r["id"], id);

        // 选中 + 移动（走选区回退）
        let r = handle(&mut doc, json!({"cmd": "select", "ids": [id]}));
        assert_eq!(r["selected"], 1);
        let r = handle(&mut doc, json!({"cmd": "move", "dx": 5.0, "dy": -5.0}));
        assert_eq!(r["moved"], 1);
        // euclid Vector2D 序列化为数组 [x, y]。
        let got = handle(&mut doc, json!({"cmd": "get", "id": id}));
        let pos_x = got["transform"]["pos"][0].as_f64().unwrap();
        assert!((pos_x - 15.0).abs() < 1e-4, "pos_x={pos_x}");

        // undo 回到 10，redo 再到 15
        assert!(handle(&mut doc, json!({"cmd": "undo"}))["performed"]
            .as_bool()
            .unwrap());
        let pos_x = handle(&mut doc, json!({"cmd": "get", "id": id}))["transform"]["pos"][0]
            .as_f64()
            .unwrap();
        assert!((pos_x - 10.0).abs() < 1e-4);
        assert!(handle(&mut doc, json!({"cmd": "redo"}))["performed"]
            .as_bool()
            .unwrap());

        // 文字
        let r = handle(
            &mut doc,
            json!({"cmd": "add_text", "text": "hello", "x": 1.0, "y": 2.0}),
        );
        assert!(r["ok"].as_bool().unwrap(), "{r}");
        assert_eq!(handle(&mut doc, json!({"cmd": "stats"}))["items"], 2);

        // 删除（显式 ids）
        let r = handle(&mut doc, json!({"cmd": "delete", "ids": [id]}));
        assert_eq!(r["deleted"], 1);

        // 未知命令 / 坏参数不 panic，返回结构化错误
        assert!(!handle(&mut doc, json!({"cmd": "nope"}))["ok"]
            .as_bool()
            .unwrap());
        assert!(
            !handle(&mut doc, json!({"cmd": "get", "id": "not-a-uuid"}))["ok"]
                .as_bool()
                .unwrap()
        );
    }

    #[test]
    fn add_item_raw_escape_hatch() {
        let mut doc = CliDoc::new();
        // ItemKind::Frame 的 JSON（与 .prz 内 kind blob 同形）
        let kind = json!({
            "Frame": {
                "base_size": [800.0, 600.0],
                "number": 1,
                "name": null,
                "follow_global_ratio": false
            }
        });
        let r = handle(
            &mut doc,
            json!({"cmd": "add_item", "kind": kind, "x": 0.0, "y": 0.0}),
        );
        assert!(r["ok"].as_bool().unwrap(), "{r}");
        let list = handle(&mut doc, json!({"cmd": "list"}));
        assert_eq!(list["items"][0]["kind"], "frame");
        assert_eq!(list["items"][0]["number"], 1);
    }
}
