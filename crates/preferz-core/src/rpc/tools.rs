//! MCP 工具表：15 个工具的 schema（`tools/list` 返回值）。
//!
//! 设计约定：
//! - 表按名称**字典序排列**（2026-07-28 spec 要求工具列表确定性排序），新增工具
//!   必须插到正确位置。
//! - 工具与 JSONL 命令一一对应：`tools/call` 把 `name` 当 `cmd`、`arguments`
//!   当请求体直接转投 [`crate::rpc::dispatch`]，两套入口共享同一套语义与
//!   undo 通道，不另写映射层。
//! - 工具集刻意精简（D 决策，见 `.agents/CHANGELOG.md` 对应条目）：
//!   `select` 传空数组 = 全不选；全选 = `list` 拿 ids 再 `select`；
//!   单元素详情 = `list` + `ids` 过滤 + `full: true`。
//! - 描述面向 LLM，用英文（与 CLI 协议文档一致；GUI 的 i18n 约定不覆盖本层）。

use std::sync::LazyLock;

use serde_json::{json, Value};

/// 一个 MCP 工具定义（name/description/inputSchema/annotations）。
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    /// `readOnlyHint`：只读工具（不改变文档状态）。
    pub read_only: bool,
    /// `destructiveHint`：破坏性工具（删除）。
    pub destructive: bool,
}

/// 工具表（字典序，勿打乱）。`json!` 非_const，静态构建走 `LazyLock`。
static TOOLS: LazyLock<Vec<ToolDef>> = LazyLock::new(|| {
    vec![
        ToolDef {
            name: "add_chart",
            description: "Create a single-series bar or line chart from two-column data. \
Provide EITHER 'data' (TSV/CSV text: label<TAB>value per line, same parser as GUI \
clipboard paste) OR 'labels' + 'values' arrays. The chart is placed with its \
top-left corner at (x, y).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "data": {
                        "type": "string",
                        "description": "Two-column TSV/CSV text: label<TAB>value per line"
                    },
                    "labels": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Category labels (alternative to 'data')"
                    },
                    "values": {
                        "type": "array",
                        "items": {"type": "number"},
                        "description": "Numeric values, same length as 'labels'"
                    },
                    "chart_type": {
                        "type": "string",
                        "enum": ["bar", "line"],
                        "description": "Chart kind, default bar"
                    },
                    "x": {"type": "number", "description": "Top-left x, default 0"},
                    "y": {"type": "number", "description": "Top-left y, default 0"}
                }
            }),
            read_only: false,
            destructive: false,
        },
        ToolDef {
            name: "add_item",
            description: "Escape hatch: create an arbitrary item from a raw ItemKind JSON object \
(same shape as stored inside .prz files). Prefer add_shape / add_text / add_mermaid \
/ add_chart when they fit.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "object",
                        "description": "ItemKind JSON, e.g. {\"Frame\": {\"base_size\": [800.0, 600.0], \"number\": 1}}"
                    },
                    "x": {"type": "number", "description": "Top-left x, default 0"},
                    "y": {"type": "number", "description": "Top-left y, default 0"}
                },
                "required": ["kind"]
            }),
            read_only: false,
            destructive: false,
        },
        ToolDef {
            name: "add_mermaid",
            description: "Create a flowchart on the canvas from a mermaid `flowchart`/`graph` DSL \
snippet. Supported: TD/TB/BT/LR/RL direction, node shapes [] () {} (( )) etc. \
(approximated as rectangle/ellipse/diamond), edges --> --- -.-> -.- ==> === <--> \
with labels (`-->|text|` or `-- text -->`), `&` branching, `<br>` line breaks. \
NOT supported: subgraph/style/classDef and non-flowchart diagram kinds. Nodes get \
an auto layered layout with orthogonal elbow connectors; the whole batch is one \
undo step. Anchor (x, y) is the top-left of the generated bounding box.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "mermaid": {
                        "type": "string",
                        "description": "Mermaid flowchart DSL, e.g. \"flowchart TD\\n a[Start] -->|yes| b{Check?}\""
                    },
                    "x": {"type": "number", "description": "Anchor top-left x, default 0"},
                    "y": {"type": "number", "description": "Anchor top-left y, default 0"},
                    "stroke": {"type": "string", "description": "Stroke/label color for all generated nodes and edges, \"#rrggbb\" or [r,g,b,a]; omitted = app theme default"},
                    "stroke_width": {"type": "number", "description": "Stroke width in canvas px; omitted = app default"}
                },
                "required": ["mermaid"]
            }),
            read_only: false,
            destructive: false,
        },
        ToolDef {
            name: "add_shape",
            description: "Create a basic shape (rectangle / ellipse / diamond) at (x, y) with \
size w×h. One undo step. Returns the new item id.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "shape": {
                        "type": "string",
                        "enum": ["rectangle", "ellipse", "diamond"],
                        "description": "Shape kind ('rect'/'circle' accepted as aliases)"
                    },
                    "x": {"type": "number", "description": "Top-left x, default 0"},
                    "y": {"type": "number", "description": "Top-left y, default 0"},
                    "w": {"type": "number", "description": "Width (also accepts 'width')"},
                    "h": {"type": "number", "description": "Height (also accepts 'height')"},
                    "stroke": {"type": "string", "description": "Stroke color, \"#rrggbb\" or [r,g,b,a]; omitted = app theme default"},
                    "stroke_width": {"type": "number", "description": "Stroke width in canvas px"},
                    "fill": {"type": "string", "description": "Fill color; omit for no fill"}
                },
                "required": ["shape", "w", "h"]
            }),
            read_only: false,
            destructive: false,
        },
        ToolDef {
            name: "add_text",
            description: "Create a free text item at (x, y). Returns the new item id.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "text": {"type": "string", "description": "Text content"},
                    "x": {"type": "number", "description": "Top-left x, default 0"},
                    "y": {"type": "number", "description": "Top-left y, default 0"},
                    "font_size": {"type": "number", "description": "Font size, default 20"},
                    "color": {"type": "string", "description": "Color, \"#rrggbb\" or [r,g,b,a]; omitted = app theme default"}
                },
                "required": ["text"]
            }),
            read_only: false,
            destructive: false,
        },
        ToolDef {
            name: "delete",
            description: "Delete items by id (falls back to the current selection when 'ids' is \
omitted). Deleting a closed shape also deletes its bound texts. One undo step.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "ids": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Item UUIDs; omit to use the current selection"
                    }
                }
            }),
            read_only: false,
            destructive: true,
        },
        ToolDef {
            name: "list",
            description: "List items, topmost first. Default entries are compact summaries \
(id, kind, position, selected, ...). Use 'ids' to fetch specific items and \
'full' to get the complete item JSON (the same representation stored in .prz \
files) — this replaces a separate get tool.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "ids": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Only list these item UUIDs (unknown id -> error)"
                    },
                    "full": {
                        "type": "boolean",
                        "description": "Return full item JSON instead of summaries, default false"
                    }
                }
            }),
            read_only: true,
            destructive: false,
        },
        ToolDef {
            name: "move",
            description: "Move items by (dx, dy) in canvas coordinates, by id or via the \
current selection. One undo step.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "ids": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Item UUIDs; omit to use the current selection"
                    },
                    "dx": {"type": "number", "description": "Delta x"},
                    "dy": {"type": "number", "description": "Delta y"}
                },
                "required": ["dx", "dy"]
            }),
            read_only: false,
            destructive: false,
        },
        ToolDef {
            name: "new",
            description: "Reset to an empty document: clears the scene, undo history and the \
current file connection.",
            input_schema: json!({"type": "object", "properties": {}}),
            read_only: false,
            destructive: false,
        },
        ToolDef {
            name: "open",
            description: "Open a .prz file, replacing the current document.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to a .prz file"}
                },
                "required": ["path"]
            }),
            read_only: false,
            destructive: false,
        },
        ToolDef {
            name: "redo",
            description: "Redo the last undone operation.",
            input_schema: json!({"type": "object", "properties": {}}),
            read_only: false,
            destructive: false,
        },
        ToolDef {
            name: "save",
            description: "Save the document. Without 'path' saves to the current file (errors \
if never saved); with a different 'path' performs save-as.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Target .prz path for save-as"}
                }
            }),
            read_only: false,
            destructive: false,
        },
        ToolDef {
            name: "select",
            description: "Set the selection to the given item ids. An empty array deselects \
everything. To select all items: call list first, then pass its ids.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "ids": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Item UUIDs to select; empty array = deselect all"
                    }
                },
                "required": ["ids"]
            }),
            read_only: false,
            destructive: false,
        },
        ToolDef {
            name: "stats",
            description: "Get document statistics: path, item/selection counts, undo/redo \
depth, next z and viewport state.",
            input_schema: json!({"type": "object", "properties": {}}),
            read_only: true,
            destructive: false,
        },
        ToolDef {
            name: "undo",
            description: "Undo the last operation.",
            input_schema: json!({"type": "object", "properties": {}}),
            read_only: false,
            destructive: false,
        },
    ]
});

/// 工具表切片（首次访问时构建，之后缓存）。
pub fn tools() -> &'static [ToolDef] {
    &TOOLS
}

/// `tools/list` 的结果体：工具数组（确定性字典序）+ 结果缓存元数据。
/// 静态表内容不变，缓存 1 小时、仅本连接可见。
pub fn tools_list_result() -> Value {
    let tools: Vec<Value> = tools()
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.input_schema,
                "annotations": {
                    "readOnlyHint": t.read_only,
                    "destructiveHint": t.destructive,
                },
            })
        })
        .collect();
    json!({
        "tools": tools,
        "ttlMs": 3_600_000,
        "cacheScope": "private",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_are_sorted_by_name() {
        let mut names: Vec<&str> = tools().iter().map(|t| t.name).collect();
        let sorted = names.clone();
        names.sort_unstable();
        assert_eq!(names, sorted, "工具表必须保持字典序（spec 确定性排序）");
    }

    #[test]
    fn has_exactly_fifteen_tools() {
        assert_eq!(tools().len(), 15);
    }

    #[test]
    fn schemas_are_objects_with_type() {
        for t in tools() {
            assert_eq!(
                t.input_schema.get("type").and_then(Value::as_str),
                Some("object"),
                "tool {} inputSchema.type",
                t.name
            );
        }
    }
}
