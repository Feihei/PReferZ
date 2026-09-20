//! mermaid 流程图子集解析 + 分层布局（plan #9）。纯函数、无副作用（core L1）。
//!
//! 拍板范围（2026-09-20）：受限自研解析器（零依赖）——首行 `flowchart|graph
//! TD|TB|LR|RL|BT`；节点 `id[标签]`（矩形）/ `id(标签)`（椭圆）/ `id{标签}`
//! （菱形）/ 裸 `id`（矩形，标签=id）；边仅支持 `-->`（可链式 `a --> b --> c`）。
//! 边标签 `|text|`、无向线 `---`、虚线/粗线箭头、subgraph 均不支持（报错并
//! 指出行号）。布局=分层（层级=最长路径深度），层间距/同层间距 100px
//! （对齐 plan #7 `FLOWCHART_GAP` 语义）。

/// 节点形状（映射到 `ShapeType` 的矩形/椭圆/菱形三类，对齐 Excalidraw
/// isFlowchartNodeElement）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MermaidShape {
    Rectangle,
    Ellipse,
    Diamond,
}

#[derive(Debug, Clone)]
pub struct MermaidNode {
    /// mermaid 节点 id（原始字符串；图内引用用 `nodes` 下标）。
    pub id: String,
    pub label: String,
    pub shape: MermaidShape,
}

/// 有向边（from/to 为 `nodes` 下标）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MermaidEdge {
    pub from: usize,
    pub to: usize,
}

#[derive(Debug, Clone)]
pub struct MermaidFlowchart {
    /// true = 主轴水平（LR/RL），false = 主轴垂直（TD/TB/BT）。
    pub horizontal: bool,
    pub nodes: Vec<MermaidNode>,
    pub edges: Vec<MermaidEdge>,
}

/// 解析 mermaid 流程图子集。`Err(消息)` 自带行号（第 n 行）。
pub fn parse_mermaid_flowchart(src: &str) -> Result<MermaidFlowchart, String> {
    let mut horizontal = false;
    let mut seen_header = false;
    let mut nodes: Vec<MermaidNode> = Vec::new();
    let mut index_of: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut edges: Vec<MermaidEdge> = Vec::new();

    for (lineno, raw) in src.lines().enumerate() {
        let n = lineno + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        if !seen_header {
            let mut parts = line.split_whitespace();
            let (Some(head), Some(dir), None) = (parts.next(), parts.next(), parts.next()) else {
                return Err(format!(
                    "第 {n} 行: 首行应为 `flowchart TD|LR` 或 `graph TD|LR`"
                ));
            };
            match head {
                "flowchart" | "graph" => {}
                other => {
                    return Err(format!(
                        "第 {n} 行: 不认识的语法 `{other}`，首行应为 flowchart/graph"
                    ));
                }
            }
            match dir {
                "TD" | "TB" | "BT" => horizontal = false,
                "LR" | "RL" => horizontal = true,
                other => {
                    return Err(format!(
                        "第 {n} 行: 不支持的方向 `{other}`（TD/TB/BT/LR/RL）"
                    ));
                }
            }
            seen_header = true;
            continue;
        }

        // 边语句（可链式）：按 `-->` 拆段
        if line.contains("-->") {
            let mut prev: Option<usize> = None;
            for seg in line.split("-->") {
                let tok = seg.trim();
                // 边标签 `|text|` 暂不支持
                if tok.contains('|') {
                    return Err(format!("第 {n} 行: 暂不支持边标签 `|…|`"));
                }
                let (id, label, shape) = parse_node_token(tok)
                    .ok_or_else(|| format!("第 {n} 行: 无法解析节点 `{tok}`"))?;
                let i = intern_node(id, label, shape, &mut nodes, &mut index_of);
                if let Some(p) = prev {
                    edges.push(MermaidEdge { from: p, to: i });
                }
                prev = Some(i);
            }
            continue;
        }

        // 其它边语法（---、-.->、==> 等）不支持
        if line.contains("---") || line.contains("-.") || line.contains("==") {
            return Err(format!("第 {n} 行: 暂不支持该边语法（仅支持 `-->`）"));
        }

        // 单行节点定义（不支持一行多个定义）
        let (id, label, shape) =
            parse_node_token(line).ok_or_else(|| format!("第 {n} 行: 无法解析节点 `{line}`"))?;
        intern_node(id, label, shape, &mut nodes, &mut index_of);
    }

    if !seen_header {
        return Err("缺少首行 `flowchart TD` 或 `graph LR`".to_string());
    }
    if nodes.is_empty() {
        return Err("流程图为空（没有节点）".to_string());
    }
    Ok(MermaidFlowchart {
        horizontal,
        nodes,
        edges,
    })
}

/// 取已注册节点下标，不存在则新建（矩形、标签=id）；随后用本次解析到的
/// label/shape 覆盖（定义处优先于裸引用的默认值）。
fn intern_node(
    id: String,
    label: Option<String>,
    shape: Option<MermaidShape>,
    nodes: &mut Vec<MermaidNode>,
    index_of: &mut std::collections::HashMap<String, usize>,
) -> usize {
    let i = match index_of.get(&id) {
        Some(&i) => i,
        None => {
            let i = nodes.len();
            nodes.push(MermaidNode {
                id: id.clone(),
                label: id.clone(),
                shape: MermaidShape::Rectangle,
            });
            index_of.insert(id, i);
            i
        }
    };
    if let Some(l) = label {
        nodes[i].label = l;
    }
    if let Some(s) = shape {
        nodes[i].shape = s;
    }
    i
}

/// 解析节点 token：`id[标签]`（矩形）/ `id(标签)`（椭圆）/ `id{标签}`（菱形）/
/// 裸 `id`（矩形）。id 取括号前的裸名；无括号时整个 token 即 id。
fn parse_node_token(tok: &str) -> Option<(String, Option<String>, Option<MermaidShape>)> {
    let tok = tok.trim();
    if tok.is_empty() {
        return None;
    }
    // 找第一个括号字符（id 与包裹体的分界）
    let open_pos = tok.find(['[', '(', '{']);
    let (id_part, shape, inner) = match open_pos {
        Some(p) => {
            let close = match tok.as_bytes()[p] {
                b'[' => ']',
                b'(' => ')',
                _ => '}',
            };
            let shape = match tok.as_bytes()[p] {
                b'[' => MermaidShape::Rectangle,
                b'(' => MermaidShape::Ellipse,
                _ => MermaidShape::Diamond,
            };
            if !tok.ends_with(close) {
                return None; // 括号不闭合
            }
            let inner = &tok[p + 1..tok.len() - 1];
            (tok[..p].trim(), Some(shape), Some(inner.trim().to_string()))
        }
        None => (tok, None, None),
    };
    // id：裸名（字母数字下划线连字符）；括号写法省略 id 时用标签清洗兜底
    let id = if id_part.is_empty() {
        sanitize_id(inner.as_ref()?)
    } else {
        if !id_part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return None;
        }
        id_part.to_string()
    };
    if id.is_empty() {
        return None;
    }
    Some((id, inner, shape))
}

/// id 清洗：非法字符替换为 `_`，空则兜底 `node`。
fn sanitize_id(s: &str) -> String {
    let cleaned: String = s
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "node".to_string()
    } else {
        cleaned
    }
}

// ─────────────────────────── 分层布局 ───────────────────────────

/// 层间距 / 同层间距（对齐 plan #7 `FLOWCHART_GAP` = 100px 语义）。
pub const MERMAID_GAP: f32 = 100.0;

/// 布局结果：节点 `index` 放到 `(x, y)`（左上角），尺寸 `(w, h)`。
#[derive(Debug, Clone)]
pub struct MermaidLayoutNode {
    pub index: usize,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// 估算节点尺寸：CJK 字符 14px、ASCII 8px 宽；高 60（菱形 90），
/// 菱形左右各多留 40px（斜边内收，避免文字出界）。
fn estimate_size(node: &MermaidNode) -> (f32, f32) {
    let text_w: f32 = node
        .label
        .chars()
        .map(|c| if c.is_ascii() { 8.0 } else { 14.0 })
        .sum();
    match node.shape {
        MermaidShape::Diamond => ((text_w + 120.0).max(140.0), 90.0),
        MermaidShape::Ellipse => ((text_w + 60.0).max(120.0), 60.0),
        MermaidShape::Rectangle => ((text_w + 40.0).max(100.0), 60.0),
    }
}

/// 分层布局：层级 = 最长路径深度（边松弛至多 n 轮，环自然有界收敛）；
/// 同层按出现顺序排布、整行在交叉轴居中。主轴 = 层方向（TD: y / LR: x）。
pub fn layout_flowchart(fc: &MermaidFlowchart) -> Vec<MermaidLayoutNode> {
    let n = fc.nodes.len();
    if n == 0 {
        return Vec::new();
    }
    // 层级松弛（最长路径）
    let mut level = vec![0usize; n];
    for _ in 0..n {
        let mut changed = false;
        for e in &fc.edges {
            if level[e.to] < level[e.from] + 1 {
                level[e.to] = level[e.from] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let sizes: Vec<(f32, f32)> = fc.nodes.iter().map(estimate_size).collect();

    // 分层（BTreeMap 保层序；层内按下标序 = 出现顺序）
    let mut layers: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for (i, &l) in level.iter().enumerate() {
        layers.entry(l).or_default().push(i);
    }
    let max_level = match layers.keys().next_back() {
        Some(&l) => l,
        None => return Vec::new(),
    };

    // 第一遍：逐层交叉轴排布，记录每层行宽与主轴起点
    // （行元素 = (节点下标, 交叉轴位置)；行记录 = (行, 层高, 主轴起点)）
    type LayoutRow = Vec<(usize, f32)>;
    type LayoutRowRecord = (LayoutRow, f32, f32);
    let mut rows: Vec<LayoutRowRecord> = Vec::new();
    let mut row_widths: Vec<f32> = Vec::new();
    let mut main_cursor = 0.0_f32;
    for li in 0..=max_level {
        let Some(members) = layers.get(&li) else {
            continue;
        };
        let layer_h = members.iter().map(|&i| sizes[i].1).fold(0.0_f32, f32::max);
        let mut cross_cursor = 0.0_f32;
        let mut row: Vec<(usize, f32)> = Vec::with_capacity(members.len());
        for &i in members {
            row.push((i, cross_cursor));
            cross_cursor += sizes[i].0 + MERMAID_GAP;
        }
        let row_w = (cross_cursor - MERMAID_GAP).max(0.0);
        row_widths.push(row_w);
        rows.push((row, layer_h, main_cursor));
        main_cursor += layer_h + MERMAID_GAP;
    }
    let max_row_w = row_widths.iter().cloned().fold(0.0_f32, f32::max);

    // 第二遍：交叉轴整行居中后落位
    let mut out: Vec<MermaidLayoutNode> = Vec::with_capacity(n);
    for (li, (row, _, main0)) in rows.iter().enumerate() {
        let cross_offset = (max_row_w - row_widths[li]) / 2.0;
        for (i, cross) in row {
            let (w, h) = sizes[*i];
            let (x, y) = if fc.horizontal {
                (*main0, cross_offset + cross)
            } else {
                (cross_offset + cross, *main0)
            };
            out.push(MermaidLayoutNode {
                index: *i,
                x,
                y,
                w,
                h,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_td_flowchart() {
        let fc = parse_mermaid_flowchart("flowchart TD\n a[开始] --> b[结束]").unwrap();
        assert!(!fc.horizontal);
        assert_eq!(fc.nodes.len(), 2);
        assert_eq!(fc.nodes[0].label, "开始");
        assert_eq!(fc.nodes[0].shape, MermaidShape::Rectangle);
        assert_eq!(fc.edges, vec![MermaidEdge { from: 0, to: 1 }]);
    }

    #[test]
    fn parses_chained_edges_and_shapes() {
        let fc = parse_mermaid_flowchart("graph LR\n s(起点) --> m{判断} --> e\n e --> s").unwrap();
        assert!(fc.horizontal);
        assert_eq!(fc.nodes.len(), 3);
        assert_eq!(fc.nodes[0].shape, MermaidShape::Ellipse);
        assert_eq!(fc.nodes[1].shape, MermaidShape::Diamond);
        assert_eq!(fc.edges.len(), 3);
        assert_eq!(fc.edges[2], MermaidEdge { from: 2, to: 0 });
    }

    #[test]
    fn bare_reference_creates_default_node() {
        let fc = parse_mermaid_flowchart("flowchart TD\n a --> b").unwrap();
        assert_eq!(fc.nodes.len(), 2);
        assert_eq!(fc.nodes[1].label, "b");
        assert_eq!(fc.nodes[1].shape, MermaidShape::Rectangle);
    }

    #[test]
    fn skips_comments_and_blank_lines() {
        let fc = parse_mermaid_flowchart("%% 注释\n\nflowchart TD\n%% 另一行\n a --> b\n").unwrap();
        assert_eq!(fc.nodes.len(), 2);
    }

    #[test]
    fn rejects_missing_header_and_bad_syntax() {
        assert!(parse_mermaid_flowchart("a --> b").is_err());
        assert!(parse_mermaid_flowchart("flowchart TD\n a --- b").is_err());
        assert!(parse_mermaid_flowchart("flowchart TD\n a -->|是| b").is_err());
        assert!(parse_mermaid_flowchart("flowchart TD\n subgraph X\n a --> b\n end").is_err());
        // 错误消息带行号
        let err = parse_mermaid_flowchart("flowchart TD\n a --- b").unwrap_err();
        assert!(err.contains("第 2 行"), "{err}");
    }

    #[test]
    fn layout_levels_follow_longest_path() {
        let fc = parse_mermaid_flowchart("flowchart TD\n a --> b\n a --> c\n b --> d\n c --> d")
            .unwrap();
        let layout = layout_flowchart(&fc);
        assert_eq!(layout.len(), 4);
        let by_idx = |i: usize| layout.iter().find(|n| n.index == i).unwrap();
        // d 的层 = 2（经 b 或 c），y 大于 b/c
        assert!((by_idx(3).y - by_idx(1).y).abs() > MERMAID_GAP);
        assert_eq!(by_idx(1).y, by_idx(2).y, "同层 y 一致");
    }

    #[test]
    fn layout_horizontal_uses_x_as_main_axis() {
        let fc = parse_mermaid_flowchart("graph LR\n a --> b").unwrap();
        let layout = layout_flowchart(&fc);
        let a = layout.iter().find(|n| n.index == 0).unwrap();
        let b = layout.iter().find(|n| n.index == 1).unwrap();
        assert!(b.x > a.x + MERMAID_GAP);
        assert_eq!(a.y, b.y, "单层 y 相同");
    }

    #[test]
    fn layout_handles_cycles_without_hang() {
        let fc = parse_mermaid_flowchart("flowchart TD\n a --> b\n b --> c\n c --> a").unwrap();
        let layout = layout_flowchart(&fc);
        assert_eq!(layout.len(), 3);
    }
}
