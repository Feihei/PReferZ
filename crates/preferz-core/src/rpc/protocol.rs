//! JSONL 协议：请求参数提取、统一响应构造，以及颜色 / item id 解析。
//!
//! 协议形态（每行一个 JSON 对象，JSONL）：
//! - 请求：`{"cmd": "<name>", ...args, "req_id": <可选回显>}`
//! - 成功：`{"ok": true, "cmd": "<name>", "req_id": <回显>, ...data}`
//! - 失败：`{"ok": false, "cmd": "<name>", "req_id": <回显>, "error": "<msg>"}`
//!
//! 回显字段刻意命名为 `req_id` 而非 `id`：`id` 是领域数据键（元素 id），
//! 同名会让回声覆盖 add_shape 等命令返回的新元素 id。
//!
//! 单条命令失败不杀死会话，agent 可在下一条命令中恢复。

use crate::item::ItemId;
use serde_json::{json, Value};

/// 构造成功响应：把 `data`（对象）与 ok/cmd/回显 id 合并。
pub fn ok(cmd: &str, req: &Value, data: Value) -> Value {
    let mut resp = json!({ "ok": true, "cmd": cmd });
    if let Some(obj) = data.as_object() {
        if let Value::Object(map) = &mut resp {
            for (k, v) in obj {
                map.insert(k.clone(), v.clone());
            }
        }
    }
    attach_id(resp, req)
}

/// 构造失败响应。
pub fn fail(cmd: &str, req: &Value, error: impl Into<String>) -> Value {
    attach_id(
        json!({ "ok": false, "cmd": cmd, "error": error.into() }),
        req,
    )
}

/// 把请求中的可选 `req_id` 原样带回响应（agent 用它配对请求 / 响应）。
fn attach_id(mut resp: Value, req: &Value) -> Value {
    if let Some(req_id) = req.get("req_id") {
        if let Value::Object(map) = &mut resp {
            map.insert("req_id".to_string(), req_id.clone());
        }
    }
    resp
}

/// 必填字符串字段。
pub fn req_str<'a>(req: &'a Value, key: &str) -> Result<&'a str, String> {
    req.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing or non-string field '{key}'"))
}

/// 可选字符串字段。
pub fn opt_str<'a>(req: &'a Value, key: &str) -> Option<&'a str> {
    req.get(key).and_then(Value::as_str)
}

/// 必填 f32 字段。
pub fn req_f32(req: &Value, key: &str) -> Result<f32, String> {
    req.get(key)
        .and_then(Value::as_f64)
        .map(|v| v as f32)
        .ok_or_else(|| format!("missing or non-number field '{key}'"))
}

/// 可选 f32 字段（缺省给默认值）。
pub fn opt_f32(req: &Value, key: &str, default: f32) -> Result<f32, String> {
    match req.get(key) {
        Some(Value::Null) | None => Ok(default),
        Some(Value::Number(n)) => n
            .as_f64()
            .map(|v| v as f32)
            .ok_or_else(|| format!("field '{key}' is not a finite number")),
        Some(_) => Err(format!("field '{key}' must be a number")),
    }
}

/// 可选 bool 字段。
pub fn opt_bool(req: &Value, key: &str, default: bool) -> bool {
    req.get(key).and_then(Value::as_bool).unwrap_or(default)
}

/// 解析单个 item id（UUID 字符串）。
pub fn parse_id(s: &str) -> Result<ItemId, String> {
    ItemId::parse_str(s).map_err(|e| format!("invalid item id '{s}': {e}"))
}

/// 解析 `ids` 字符串数组；字段缺失 / null 返回 `None`（调用方自行回退到选区）。
pub fn opt_ids(req: &Value) -> Result<Option<Vec<ItemId>>, String> {
    match req.get("ids") {
        Some(Value::Array(arr)) => {
            let mut ids = Vec::with_capacity(arr.len());
            for v in arr {
                let s = v
                    .as_str()
                    .ok_or_else(|| "every entry of 'ids' must be a string".to_string())?;
                ids.push(parse_id(s)?);
            }
            Ok(Some(ids))
        }
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err("field 'ids' must be an array of UUID strings".to_string()),
    }
}

/// 解析 RGBA 颜色：`"#rgb"` / `"#rrggbb"` / `"#rrggbbaa"`（# 可省略），
/// 或数组 `[r,g,b]` / `[r,g,b,a]`（0–255）。
pub fn parse_color(v: &Value) -> Result<[u8; 4], String> {
    match v {
        Value::String(s) => parse_hex_color(s.trim_start_matches('#')),
        Value::Array(arr) => {
            let comps: Result<Vec<u8>, String> = arr
                .iter()
                .map(|c| {
                    c.as_f64()
                        .map(|n| n.clamp(0.0, 255.0) as u8)
                        .ok_or_else(|| "color array entries must be numbers".to_string())
                })
                .collect();
            let comps = comps?;
            match comps.len() {
                3 => Ok([comps[0], comps[1], comps[2], 255]),
                4 => Ok([comps[0], comps[1], comps[2], comps[3]]),
                _ => Err("color array must have 3 (rgb) or 4 (rgba) entries".to_string()),
            }
        }
        _ => Err("color must be a hex string or an [r,g,b,a] array".to_string()),
    }
}

fn parse_hex_color(hex: &str) -> Result<[u8; 4], String> {
    let mut chars = hex.chars();
    let nibbles: Vec<u8> = chars
        .by_ref()
        .map(|c| {
            c.to_digit(16)
                .map(|d| d as u8)
                .ok_or_else(|| format!("invalid hex digit '{c}' in color"))
        })
        .collect::<Result<_, _>>()?;
    match nibbles.len() {
        // rgb → 每位置双份
        3 => Ok([nibbles[0] * 17, nibbles[1] * 17, nibbles[2] * 17, 255]),
        6 => Ok([
            (nibbles[0] << 4) | nibbles[1],
            (nibbles[2] << 4) | nibbles[3],
            (nibbles[4] << 4) | nibbles[5],
            255,
        ]),
        8 => Ok([
            (nibbles[0] << 4) | nibbles[1],
            (nibbles[2] << 4) | nibbles[3],
            (nibbles[4] << 4) | nibbles[5],
            (nibbles[6] << 4) | nibbles[7],
        ]),
        n => Err(format!("hex color must be 3/6/8 hex digits, got {n}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colors() {
        assert_eq!(parse_color(&json!("#f00")).unwrap(), [255, 0, 0, 255]);
        assert_eq!(parse_color(&json!("1e1e1e")).unwrap(), [30, 30, 30, 255]);
        assert_eq!(parse_color(&json!("#00ff0080")).unwrap(), [0, 255, 0, 128]);
    }

    #[test]
    fn array_colors() {
        assert_eq!(
            parse_color(&json!([10, 20, 30])).unwrap(),
            [10, 20, 30, 255]
        );
        assert_eq!(
            parse_color(&json!([10.0, 20.0, 30.0, 0.0])).unwrap(),
            [10, 20, 30, 0]
        );
        assert!(parse_color(&json!([1, 2])).is_err());
    }

    #[test]
    fn id_echo_roundtrip() {
        let req = json!({"cmd": "ping", "req_id": 42});
        let resp = ok("ping", &req, json!({"pong": true}));
        assert_eq!(resp["req_id"], 42);
        assert_eq!(resp["ok"], true);
        assert_eq!(resp["pong"], true);
    }
}
