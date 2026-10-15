//! 请求分发：解析 `cmd` 并在实现 [`RpcDoc`] 的文档上执行，返回统一 JSON 响应。
//!
//! 所有写操作都经 `session.push(Command)` 走 undo 通道，与 GUI 共享同一套
//! core 命令语义；headless 调用方没有"拖拽预览直改"阶段，故 `MoveItems`
//! 等默认 preview 的命令必须显式 `with_preview_applied(false)`。

use crate::commands::{AddItem, AddItems, DeleteItems, MoveItems};
use crate::item::{ChartType, Item, ItemId, ItemKind};
use crate::rpc::doc::RpcDoc;
use crate::rpc::protocol::{self, ok, opt_bool, opt_f32, opt_str, parse_color, req_f32, req_str};
use crate::shape::{FontFamily, Sloppiness, StrokeStyle};
use crate::spaces::CanvasVector;
use crate::transform::Transform;
use serde_json::{json, Value};

/// 分发一条已解析为 JSON 值的请求。永不 panic：任何错误都收敛成失败响应。
pub fn dispatch(doc: &mut impl RpcDoc, req: &Value) -> Value {
    let Some(cmd) = req.get("cmd").and_then(Value::as_str) else {
        return protocol::fail("<none>", req, "request must be an object with string 'cmd'");
    };
    match run(doc, cmd, req) {
        Ok(data) => ok(cmd, req, data),
        Err(msg) => protocol::fail(cmd, req, msg),
    }
}

fn run(doc: &mut impl RpcDoc, cmd: &str, req: &Value) -> Result<Value, String> {
    match cmd {
        "ping" => Ok(json!({ "pong": true, "version": env!("CARGO_PKG_VERSION") })),
        "version" => Ok(json!({
            "version": env!("CARGO_PKG_VERSION"),
            "format": "prz"
        })),

        "new" => {
            doc.reset();
            Ok(json!({ "cleared": true }))
        }
        "open" => {
            let path = req_str(req, "path")?;
            let count = doc.open_path(std::path::Path::new(path))?;
            Ok(json!({ "path": path, "items": count }))
        }
        "save" => {
            let target = opt_str(req, "path").map(std::path::Path::new);
            let saved = doc.save_path(target)?;
            Ok(json!({ "path": saved.to_string_lossy() }))
        }

        "stats" => Ok(stats(doc)),
        "list" => list(doc, req),

        "select" => {
            let ids = protocol::opt_ids(req)?.ok_or_else(|| "missing 'ids' array".to_string())?;
            if !opt_bool(req, "additive", false) {
                doc.session().scene.deselect_all();
            }
            let mut selected = 0usize;
            for id in ids {
                if doc.session().scene.get_item(&id).is_some() {
                    doc.session().scene.select(id);
                    selected += 1;
                }
            }
            Ok(json!({ "selected": selected }))
        }
        "select_all" => {
            doc.session().scene.select_all();
            Ok(json!({ "selected": doc.session().scene.selection.len() }))
        }
        "deselect" => {
            doc.session().scene.deselect_all();
            Ok(json!({ "selected": 0 }))
        }

        "add_shape" => add_shape(doc, req).map(|id| json!({ "id": id.to_string() })),
        "add_text" => add_text(doc, req).map(|id| json!({ "id": id.to_string() })),
        "add_item" => add_item_raw(doc, req).map(|id| json!({ "id": id.to_string() })),
        "add_mermaid" => add_mermaid(doc, req),
        "add_chart" => add_chart(doc, req),

        "delete" => {
            let ids = target_ids(doc, req)?;
            if ids.is_empty() {
                return Err("no ids given and selection is empty".to_string());
            }
            let n = ids.len();
            doc.session().push(Box::new(DeleteItems::new(ids)));
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
            doc.session().push(Box::new(
                MoveItems::new(ids, CanvasVector::new(dx, dy)).with_preview_applied(false),
            ));
            Ok(json!({ "moved": n, "dx": dx, "dy": dy }))
        }

        "undo" => Ok(json!({ "performed": doc.session().undo() })),
        "redo" => Ok(json!({ "performed": doc.session().redo() })),

        other => Err(format!("unknown command '{other}'")),
    }
}

/// 命令的目标 id 列表：显式 `ids` 优先，否则回退到当前选区。
fn target_ids(doc: &mut impl RpcDoc, req: &Value) -> Result<Vec<ItemId>, String> {
    match protocol::opt_ids(req)? {
        Some(ids) => Ok(ids),
        None => Ok(doc.session().scene.selection.iter().copied().collect()),
    }
}

fn stats(doc: &mut impl RpcDoc) -> Value {
    let path = doc.path().map(|p| p.to_string_lossy().to_string());
    let s = doc.session();
    json!({
        "path": path,
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

/// list：每个 item 一条摘要（z 序倒序，顶层在前）。参数：
/// - `ids`（可选，UUID 数组）：只列出这些元素；任一 id 不存在则整体报错
///   （原 `get` 命令的语义并入此处：`{"cmd":"list","ids":[..],"full":true}`）；
/// - `full`（可选，默认 false）：true 时返回完整 Item JSON（与 `.prz` 内表示同源）。
fn list(doc: &mut impl RpcDoc, req: &Value) -> Result<Value, String> {
    let filter = protocol::opt_ids(req)?;
    if let Some(ids) = &filter {
        for id in ids {
            if doc.session().scene.get_item(id).is_none() {
                return Err(format!("no item with id {id}"));
            }
        }
    }
    let full = opt_bool(req, "full", false);
    let session = doc.session();
    let items: Vec<Value> = session
        .scene
        .items_by_z_order()
        .into_iter()
        .rev()
        .filter(|item| filter.as_ref().is_none_or(|ids| ids.contains(&item.id)))
        .map(|item| {
            if full {
                serde_json::to_value(item).map_err(|e| format!("serialize item failed: {e}"))
            } else {
                Ok(summarize_item(session, item))
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(json!({ "items": items }))
}

/// 每个 item 一行精简摘要（供 [`list`] 的默认精简模式）。
fn summarize_item(session: &crate::session::Session, item: &Item) -> Value {
    let mut v = json!({
        "id": item.id.to_string(),
        "kind": kind_label(&item.kind),
        "z": item.z,
        "x": item.transform.pos.x,
        "y": item.transform.pos.y,
        "selected": session.scene.selection_contains(&item.id),
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
}

fn add_shape(doc: &mut impl RpcDoc, req: &Value) -> Result<ItemId, String> {
    let shape = match req_str(req, "shape")?.to_ascii_lowercase().as_str() {
        "rectangle" | "rect" => crate::shape::ShapeType::Rectangle,
        "ellipse" | "circle" => crate::shape::ShapeType::Ellipse,
        "diamond" => crate::shape::ShapeType::Diamond,
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
    doc.session().push(Box::new(AddItem::new(item)));
    Ok(id)
}

fn add_text(doc: &mut impl RpcDoc, req: &Value) -> Result<ItemId, String> {
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
    doc.session().push(Box::new(AddItem::new(item)));
    Ok(id)
}

/// 通用转义舱：直接传 `kind`（ItemKind 的 JSON 形态，与 `.prz` 内 kind blob 同源）
/// 创建任意类型元素，可选 `x`/`y` 指定位置。
fn add_item_raw(doc: &mut impl RpcDoc, req: &Value) -> Result<ItemId, String> {
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
    doc.session().push(Box::new(AddItem::new(item)));
    Ok(id)
}

/// mermaid 流程图 → 分层布局元素批（节点+绑定文字+Elbow 连接器），
/// 整批一条 `AddItems` undo。锚点：`x`/`y`（缺省 0,0）为生成内容包围盒左上角。
/// 风格与 GUI「草绘」出厂预设同档（Artist 手绘 + 手写体标签），GUI 打开观感一致。
fn add_mermaid(doc: &mut impl RpcDoc, req: &Value) -> Result<Value, String> {
    let src = req_str(req, "mermaid")?;
    let fc = crate::mermaid::parse_mermaid_flowchart(src)?;
    let layout = crate::mermaid::layout_flowchart(&fc);
    // 布局包围盒左上角 → 把锚点平移进去
    let (mut bx0, mut by0) = (f32::MAX, f32::MAX);
    for ln in &layout {
        bx0 = bx0.min(ln.x);
        by0 = by0.min(ln.y);
    }
    let x = opt_f32(req, "x", 0.0)?;
    let y = opt_f32(req, "y", 0.0)?;
    let style = crate::mermaid::FlowchartStyle {
        stroke: StrokeStyle::default(),
        sloppiness: Sloppiness::Artist,
        font_family: FontFamily::Handwriting,
        label_font_size: 18.0,
    };
    let built = crate::mermaid::build_flowchart_items(&fc, &layout, (x - bx0, y - by0), &style);
    let nodes = fc.nodes.len();
    let edges = fc.edges.len();
    let ids: Vec<String> = built.items.iter().map(|it| it.id.to_string()).collect();
    doc.session().push(Box::new(AddItems::new(built.items)));
    Ok(json!({ "nodes": nodes, "edges": edges, "ids": ids }))
}

/// 两列数据 → 单系列图表（柱状/折线）。数据源二选一：
/// `data`（TSV/CSV 文本，与 GUI 剪贴板路径同一解析器）或 `labels`+`values` 数组。
fn add_chart(doc: &mut impl RpcDoc, req: &Value) -> Result<Value, String> {
    let (labels, values) = match req.get("data") {
        Some(Value::Null) | None => {
            let labels = req
                .get("labels")
                .and_then(Value::as_array)
                .ok_or("missing 'data' string or 'labels' array")?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| "every entry of 'labels' must be a string".to_string())
                })
                .collect::<Result<Vec<String>, String>>()?;
            let values = req
                .get("values")
                .and_then(Value::as_array)
                .ok_or("missing 'data' string or 'labels'/'values' arrays")?
                .iter()
                .map(|v| {
                    v.as_f64()
                        .map(|n| n as f32)
                        .ok_or_else(|| "every entry of 'values' must be a number".to_string())
                })
                .collect::<Result<Vec<f32>, String>>()?;
            (labels, values)
        }
        Some(text) => crate::parse_two_column_data(text.as_str().ok_or("'data' must be a string")?)
            .ok_or_else(|| "could not parse 'data' as two-column TSV/CSV".to_string())?,
    };
    if labels.is_empty() {
        return Err("chart needs at least one data point".to_string());
    }
    if labels.len() != values.len() {
        return Err(format!(
            "labels ({}) and values ({}) lengths differ",
            labels.len(),
            values.len()
        ));
    }
    if values.iter().any(|v| !v.is_finite()) {
        return Err("values must be finite numbers".to_string());
    }
    let chart_type = match opt_str(req, "chart_type").unwrap_or("bar") {
        "bar" => ChartType::Bar,
        "line" => ChartType::Line,
        other => {
            return Err(format!(
                "unknown chart_type '{other}' (supported: bar, line)"
            ))
        }
    };
    let x = opt_f32(req, "x", 0.0)?;
    let y = opt_f32(req, "y", 0.0)?;
    let points = labels.len();
    let item = Item::new_chart(chart_type, labels, values, x, y);
    let id = item.id;
    doc.session().push(Box::new(AddItem::new(item)));
    Ok(json!({ "id": id.to_string(), "points": points }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::test_support::TestDoc;
    use serde_json::json;

    fn handle(doc: &mut TestDoc, v: Value) -> Value {
        dispatch(doc, &v)
    }

    #[test]
    fn full_headless_workflow() {
        let mut doc = TestDoc::new();

        let r = handle(&mut doc, json!({"cmd": "ping"}));
        assert!(r["ok"].as_bool().unwrap());

        // 加一个矩形
        let r = handle(
            &mut doc,
            json!({"cmd": "add_shape", "shape": "rect", "x": 10.0, "y": 20.0, "w": 100.0, "h": 50.0}),
        );
        assert!(r["ok"].as_bool().unwrap(), "{r}");
        let id = r["id"].as_str().unwrap().to_string();

        // list / stats / get（full 单条）
        let r = handle(&mut doc, json!({"cmd": "list"}));
        assert_eq!(r["items"].as_array().unwrap().len(), 1);
        assert_eq!(r["items"][0]["kind"], "shape");
        assert_eq!(r["items"][0]["shape"], "Rectangle");

        let r = handle(&mut doc, json!({"cmd": "stats"}));
        assert_eq!(r["items"], 1);
        assert_eq!(r["undo_depth"], 1);

        let r = handle(&mut doc, json!({"cmd": "list", "ids": [id], "full": true}));
        assert_eq!(r["items"].as_array().unwrap().len(), 1);
        assert_eq!(r["items"][0]["id"], id);
        assert!(r["items"][0]["transform"].is_object());
        // list ids 过滤遇到不存在的 id → 报错（get 语义并入）
        assert!(
            !handle(&mut doc, json!({"cmd": "list", "ids": ["not-a-uuid"]}))["ok"]
                .as_bool()
                .unwrap()
        );

        // 选中 + 移动（走选区回退）
        let r = handle(&mut doc, json!({"cmd": "select", "ids": [id]}));
        assert_eq!(r["selected"], 1);
        let r = handle(&mut doc, json!({"cmd": "move", "dx": 5.0, "dy": -5.0}));
        assert_eq!(r["moved"], 1);
        // euclid Vector2D 序列化为数组 [x, y]。
        let got = handle(&mut doc, json!({"cmd": "list", "ids": [id], "full": true}));
        let pos_x = got["items"][0]["transform"]["pos"][0].as_f64().unwrap();
        assert!((pos_x - 15.0).abs() < 1e-4, "pos_x={pos_x}");

        // undo 回到 10，redo 再到 15
        assert!(handle(&mut doc, json!({"cmd": "undo"}))["performed"]
            .as_bool()
            .unwrap());
        let pos_x = handle(&mut doc, json!({"cmd": "list", "ids": [id], "full": true}))["items"][0]
            ["transform"]["pos"][0]
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
            !handle(&mut doc, json!({"cmd": "list", "ids": ["not-a-uuid"]}))["ok"]
                .as_bool()
                .unwrap()
        );
    }

    #[test]
    fn open_save_unsupported_yields_structured_error() {
        let mut doc = TestDoc::new();
        let r = handle(&mut doc, json!({"cmd": "open", "path": "x.prz"}));
        assert!(!r["ok"].as_bool().unwrap());
        let r = handle(&mut doc, json!({"cmd": "save"}));
        assert!(!r["ok"].as_bool().unwrap());
    }

    #[test]
    fn add_mermaid_builds_flowchart_and_undoes() {
        let mut doc = TestDoc::new();
        let r = handle(
            &mut doc,
            json!({"cmd": "add_mermaid", "mermaid": "flowchart TD\n a[开始] -->|是| b{判断}"}),
        );
        assert!(r["ok"].as_bool().unwrap(), "{r}");
        assert_eq!(r["nodes"], 2);
        assert_eq!(r["edges"], 1);
        // 2 节点 + 2 绑定文字 + 1 连接器
        assert_eq!(r["ids"].as_array().unwrap().len(), 5);
        assert_eq!(handle(&mut doc, json!({"cmd": "stats"}))["items"], 5);
        // 坏语法报错不 panic
        let r = handle(
            &mut doc,
            json!({"cmd": "add_mermaid", "mermaid": "subgraph x"}),
        );
        assert!(!r["ok"].as_bool().unwrap());
        // 一条 undo 撤掉整批
        assert!(handle(&mut doc, json!({"cmd": "undo"}))["performed"]
            .as_bool()
            .unwrap());
        assert_eq!(handle(&mut doc, json!({"cmd": "stats"}))["items"], 0);
    }

    #[test]
    fn add_chart_from_data_text_and_arrays() {
        let mut doc = TestDoc::new();
        // TSV 文本（与 GUI 剪贴板同源解析器）
        let r = handle(
            &mut doc,
            json!({"cmd": "add_chart", "data": "一月\t10\n二月\t20", "chart_type": "line"}),
        );
        assert!(r["ok"].as_bool().unwrap(), "{r}");
        assert_eq!(r["points"], 2);
        // labels/values 数组
        let r = handle(
            &mut doc,
            json!({"cmd": "add_chart", "labels": ["a", "b"], "values": [1, 2]}),
        );
        assert!(r["ok"].as_bool().unwrap(), "{r}");
        // 长度不一致 / 坏数据 / 未知类型报错
        assert!(!handle(
            &mut doc,
            json!({"cmd": "add_chart", "labels": ["a"], "values": [1, 2]})
        )["ok"]
            .as_bool()
            .unwrap());
        assert!(
            !handle(&mut doc, json!({"cmd": "add_chart", "data": "not numbers"}))["ok"]
                .as_bool()
                .unwrap()
        );
        assert!(!handle(
            &mut doc,
            json!({"cmd": "add_chart", "labels": ["a"], "values": [1], "chart_type": "pie"})
        )["ok"]
            .as_bool()
            .unwrap());
    }

    #[test]
    fn add_item_raw_escape_hatch() {
        let mut doc = TestDoc::new();
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
