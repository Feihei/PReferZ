//! mermaid 流程图子集解析 + 分层布局（plan #9；plan #17 语法完整化）。
//! 纯函数、无副作用（core L1、零依赖）。
//!
//! 支持首行 `flowchart|graph TD|TB|LR|RL|BT`；节点 `id[标签]` 及其括号外形
//! 变体（详见 [`parse_node_token`]，冷门形按 DP1 近似映射到矩形/椭圆/菱形三类）；
//! 边支持箭头族 `-->` / `---` / `-.->` / `-.-` / `==>` / `===` / `<-->`，
//! 两种边标签写法 `-->|文本|`（pipe）与 `-- 文本 -->`（inline，仅实线），
//! 一行多分支 `&`（`a --> b & c`、`a & b --> c & d` 交叉积），以及引号标签
//! `["含 空格"]`。`subgraph` / `direction` / `style` / `classDef` 等分组与
//! 样式语句、以及 sequence/class/state 等**非 flowchart 图种均不支持**（报错
//! 并指出行号，DP 见 `.agents/plan.md` §17）。布局 = 分层（层级 = 最长路径深度），
//! 层间距 / 同层间距 100px（对齐 plan #7 `FLOWCHART_GAP` 语义）。

/// 节点形状。DP1「全部近似映射」：mermaid 的多种括号外形在解析期即收敛到这三类
/// （映射 `ShapeType` 的矩形/椭圆/菱形，对齐 Excalidraw isFlowchartNodeElement）。
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

/// 箭头族（边线型），映射到二进制层的 (start/end `ArrowHeadStyle`, `DashStyle`, width)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MermaidArrow {
    /// `-->` 实线箭头。
    Arrow,
    /// `---` 实线无箭头（open link）。
    Open,
    /// `-.->` 虚线箭头。
    Dotted,
    /// `-.-` 虚线无箭头。
    DottedOpen,
    /// `==>` 粗线箭头。
    Thick,
    /// `===` 粗线无箭头。
    ThickOpen,
    /// `<-->` 双向实线箭头。
    Double,
}

/// 有向边（from/to 为 `nodes` 下标）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MermaidEdge {
    pub from: usize,
    pub to: usize,
    pub kind: MermaidArrow,
    /// 边标签（`-->|文本|` / `-- 文本 -->`），无则 `None`。
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MermaidFlowchart {
    /// true = 主轴水平（LR/RL），false = 主轴垂直（TD/TB/BT）。
    pub horizontal: bool,
    pub nodes: Vec<MermaidNode>,
    pub edges: Vec<MermaidEdge>,
}

/// 解析结果累加器，避免函数签名携带三四个 `&mut`。
struct ParseCtx {
    nodes: Vec<MermaidNode>,
    index_of: std::collections::HashMap<String, usize>,
    edges: Vec<MermaidEdge>,
}

impl ParseCtx {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            index_of: std::collections::HashMap::new(),
            edges: Vec::new(),
        }
    }

    /// 取已注册节点下标，不存在则新建（矩形、标签=id）；`label`/`shape` 为 `Some`
    /// 时覆盖（定义处优先于裸引用的默认值）。
    fn intern(&mut self, id: String, label: Option<String>, shape: Option<MermaidShape>) -> usize {
        let i = match self.index_of.get(&id) {
            Some(&i) => i,
            None => {
                let i = self.nodes.len();
                self.nodes.push(MermaidNode {
                    id: id.clone(),
                    label: id.clone(),
                    shape: MermaidShape::Rectangle,
                });
                self.index_of.insert(id, i);
                i
            }
        };
        if let Some(l) = label {
            self.nodes[i].label = l;
        }
        if let Some(s) = shape {
            self.nodes[i].shape = s;
        }
        i
    }
}

/// 不支持的图种 / 语句关键字（首词命中即报错，指路后续）。
const UNSUPPORTED_KEYWORDS: &[&str] = &[
    "subgraph",
    "end",
    "direction",
    "style",
    "classDef",
    "class",
    "click",
    "callback",
    "linkStyle",
];

/// 解析 mermaid 流程图子集。`Err(消息)` 自带行号（第 n 行）。
///
/// 图种分派点：此处仅处理 `flowchart`/`graph`；将来加 sequence/class/state 等新
/// 图种 = 在首行分派处新增分支 + 独立布局模块，不改本函数（架构预留，见 plan §17）。
pub fn parse_mermaid_flowchart(src: &str) -> Result<MermaidFlowchart, String> {
    let mut horizontal = false;
    let mut seen_header = false;
    let mut ctx = ParseCtx::new();

    for (lineno, raw) in src.lines().enumerate() {
        let n = lineno + 1;
        // 行内 `%%` 注释截断 + 去行尾 `;` + trim。
        let line = raw.split_once("%%").map_or(raw, |(s, _)| s);
        let line = line.trim().trim_end_matches(';').trim();
        if line.is_empty() {
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

        if let Some(word) = line.split_whitespace().next() {
            if UNSUPPORTED_KEYWORDS.contains(&word) {
                return Err(format!("第 {n} 行: 暂不支持 `{word}` 语句"));
            }
        }
        parse_statement(line, n, &mut ctx)?;
    }

    if !seen_header {
        return Err("缺少首行 `flowchart TD` 或 `graph LR`".to_string());
    }
    if ctx.nodes.is_empty() {
        return Err("流程图为空（没有节点）".to_string());
    }
    Ok(MermaidFlowchart {
        horizontal,
        nodes: ctx.nodes,
        edges: ctx.edges,
    })
}

/// 解析一条语句（已 trim、去注释、非空）。文法：
/// `GROUP (LINK GROUP)*`，`GROUP = NODE (& NODE)*`，相邻组对每条 LINK 做交叉积。
fn parse_statement(line: &str, n: usize, ctx: &mut ParseCtx) -> Result<(), String> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut links: Vec<(MermaidArrow, Option<String>)> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut i = 0usize;
    let mut expect_node = true;

    loop {
        skip_ws(line, &mut i);
        if i >= line.len() {
            break;
        }
        if starts_with(line, i, "&") {
            // `&` 只在「刚解析完一个节点」后合法（同组内并列），其后需再接一个节点。
            if expect_node {
                return Err(format!("第 {n} 行: `&` 两侧需为节点"));
            }
            i += 1;
            expect_node = true;
            continue;
        }
        if let Some((kind, label, ni)) = scan_link(line, i) {
            if expect_node {
                return Err(format!("第 {n} 行: 箭头缺少源节点"));
            }
            links.push((kind, label));
            groups.push(std::mem::take(&mut current));
            i = ni;
            expect_node = true;
            continue;
        }
        if let Some((id, label, shape, ni)) = scan_node(line, i) {
            if !expect_node {
                return Err(format!("第 {n} 行: 语句结构异常（相邻两段节点缺分隔）"));
            }
            let gi = ctx.intern(id, label, shape);
            current.push(gi);
            i = ni;
            expect_node = false;
            continue;
        }
        return Err(format!(
            "第 {n} 行: 无法解析（自第 {} 字符起 `{}`）",
            i + 1,
            &line[i..]
        ));
    }

    if expect_node && groups.is_empty() {
        return Err(format!("第 {n} 行: 无法解析该语句"));
    }
    if !expect_node && !current.is_empty() {
        groups.push(current);
    }
    if links.is_empty() {
        // 纯节点定义行（无箭头）：已在 intern 时注册，无副作用。
        return Ok(());
    }
    for (li, (kind, label)) in links.iter().enumerate() {
        let srcs = &groups[li];
        let dsts = &groups[li + 1];
        for &s in srcs {
            for &d in dsts {
                ctx.edges.push(MermaidEdge {
                    from: s,
                    to: d,
                    kind: *kind,
                    label: label.clone(),
                });
            }
        }
    }
    Ok(())
}

fn starts_with(s: &str, i: usize, pat: &str) -> bool {
    s[i..].starts_with(pat)
}

fn skip_ws(s: &str, i: &mut usize) {
    let b = s.as_bytes();
    while *i < b.len() && matches!(b[*i], b' ' | b'\t') {
        *i += 1;
    }
}

/// 在 `s[i]` 起扫描一条链（箭头）。成功返回 (kind, 可选标签, 下一个字节偏移)。
/// 覆盖：完整箭头算子（+ 可选 `|标签|`）、`<-->` 双向、`-- 文本 -->` inline（仅实线）。
fn scan_link(s: &str, i: usize) -> Option<(MermaidArrow, Option<String>, usize)> {
    // 双向 `<-->`（DP：仅实线双向）。
    if starts_with(s, i, "<-->") {
        let j = i + 4;
        let (label, j) = scan_pipe_label(s, j);
        return Some((MermaidArrow::Double, label, j));
    }
    // 具体算子，长/特殊在前。
    let (kind, j) = if starts_with(s, i, "-->") {
        (MermaidArrow::Arrow, i + 3)
    } else if starts_with(s, i, "-.->") {
        (MermaidArrow::Dotted, i + 4)
    } else if starts_with(s, i, "==>") {
        (MermaidArrow::Thick, i + 3)
    } else if starts_with(s, i, "-.-") {
        (MermaidArrow::DottedOpen, i + 3)
    } else if starts_with(s, i, "===") {
        (MermaidArrow::ThickOpen, i + 3)
    } else if starts_with(s, i, "---") {
        (MermaidArrow::Open, i + 3)
    } else {
        // inline 实线 `-- 文本 -->`：以 `--` 起且非上述算子。
        if starts_with(s, i, "--") {
            let after = i + 2;
            let b = s.as_bytes();
            // 紧跟 `>` 会被前面 `-->` 吃掉；紧跟 `-` 会被 `---` 吃掉；故此处为 inline 起始。
            if after < b.len() && b[after] == b'>' {
                return None;
            }
            let rest = &s[after..];
            let k = rest.find("-->")?;
            let text = &rest[..k];
            if text.trim().is_empty() {
                return None;
            }
            let j = after + k + 3;
            return Some((MermaidArrow::Arrow, Some(text.trim().to_string()), j));
        }
        return None;
    };
    let (label, j) = scan_pipe_label(s, j);
    Some((kind, label, j))
}

/// 可选的 `|标签|` pipe 写法；命中则返回 (Some(标签), 新偏移)，否则原样。
fn scan_pipe_label(s: &str, i: usize) -> (Option<String>, usize) {
    let b = s.as_bytes();
    if i < b.len() && b[i] == b'|' {
        if let Some(rel) = s[i + 1..].find('|') {
            let text = s[i + 1..i + 1 + rel].trim();
            return (Some(text.to_string()), i + 1 + rel + 1);
        }
    }
    (None, i)
}

fn is_id_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-'
}

/// 在 `s[i]` 起扫描一个节点。返回 (id, 可选标签, 可选形状, 下一个字节偏移)。
fn scan_node(s: &str, i: usize) -> Option<(String, Option<String>, Option<MermaidShape>, usize)> {
    let b = s.as_bytes();
    let id_start = i;
    let mut k = i;
    while k < b.len() && is_id_char(b[k]) {
        k += 1;
    }
    // 尾部 `-` 归链（`a-->b` 里 `a-` 会误吞 `-`）：回退连续尾 `-`。
    while k > id_start && b[k - 1] == b'-' {
        k -= 1;
    }
    let id = s[id_start..k].trim().to_string();
    let after_id = k;
    // 从 after_id 起找括号体（id 与体之间可能有空格）。
    let mut p = after_id;
    skip_ws(s, &mut p);
    if p < b.len() && matches!(b[p], b'[' | b'(' | b'{' | b'>') {
        let (inner, shape, end) = parse_shape_body(s, p)?;
        let label = Some(inner);
        let id = if id.is_empty() {
            sanitize_id(label.as_deref()?)
        } else {
            id
        };
        Some((id, label, Some(shape), end))
    } else {
        if id.is_empty() {
            return None;
        }
        Some((id, None, None, after_id))
    }
}

/// 解析括号外形体（DP1 近似映射 + DP3 语义校正）。返回 (标签, 形状, 结束偏移)。
fn parse_shape_body(s: &str, i: usize) -> Option<(String, MermaidShape, usize)> {
    let close_after =
        |open: &str, close: &str, shape: MermaidShape| -> Option<(String, MermaidShape, usize)> {
            if !starts_with(s, i, open) {
                return None;
            }
            let inner_start = i + open.len();
            let rel = s[inner_start..].find(close)?;
            let inner = s[inner_start..inner_start + rel].trim();
            Some((unquote(inner), shape, inner_start + rel + close.len()))
        };
    // 双括号 / 特殊形在前（DP1：冷门形近似收敛）。
    close_after("((", "))", MermaidShape::Ellipse) // 圆
        .or_else(|| close_after("([", "])", MermaidShape::Ellipse)) // 体育场 → 近似圆
        .or_else(|| close_after("[[", "]]", MermaidShape::Rectangle)) // 子程序 → 矩形
        .or_else(|| close_after("[(", ")]", MermaidShape::Rectangle)) // 柱/数据库 → 矩形
        .or_else(|| close_after("{{", "}}", MermaidShape::Rectangle)) // 六边 → 矩形
        .or_else(|| close_after("[/", "/]", MermaidShape::Rectangle)) // 平行四边形
        .or_else(|| close_after("[\\", "\\]", MermaidShape::Rectangle)) // 反平行四边形
        .or_else(|| close_after("[/", "\\]", MermaidShape::Rectangle)) // 梯形
        .or_else(|| close_after("[\\", "/]", MermaidShape::Rectangle)) // 反梯形
        .or_else(|| close_after(">", "]", MermaidShape::Rectangle)) // 旗形
        // 单括号（DP3：`()` 圆角矩形近似为 Rectangle；`{}` 菱形）。
        .or_else(|| close_after("[", "]", MermaidShape::Rectangle))
        .or_else(|| close_after("(", ")", MermaidShape::Rectangle))
        .or_else(|| close_after("{", "}", MermaidShape::Diamond))
}

/// 引号标签：首尾同为 `"` 或 `'` 且长度 ≥ 2 → 去引号（保留内部空格/符号）；否则 trim。
fn unquote(inner: &str) -> String {
    let b = inner.as_bytes();
    if b.len() >= 2 {
        let (f, l) = (b[0], b[b.len() - 1]);
        if (f == b'"' && l == b'"') || (f == b'\'' && l == b'\'') {
            return inner[1..inner.len() - 1].to_string();
        }
    }
    inner.to_string()
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
        assert_eq!(
            fc.edges,
            vec![MermaidEdge {
                from: 0,
                to: 1,
                kind: MermaidArrow::Arrow,
                label: None
            }]
        );
    }

    #[test]
    fn parses_chained_edges_and_shapes() {
        // DP3：`()→圆角矩形近似为 Rectangle`；`{}`=菱形；`((..))`=圆→Ellipse。
        let fc = parse_mermaid_flowchart("graph LR\n s(起点) --> m{判断} --> e\n e --> s").unwrap();
        assert!(fc.horizontal);
        assert_eq!(fc.nodes.len(), 3);
        assert_eq!(fc.nodes[0].shape, MermaidShape::Rectangle);
        assert_eq!(fc.nodes[1].shape, MermaidShape::Diamond);
        assert_eq!(fc.edges.len(), 3);
        assert_eq!(
            fc.edges[2],
            MermaidEdge {
                from: 2,
                to: 0,
                kind: MermaidArrow::Arrow,
                label: None
            }
        );
    }

    #[test]
    fn parses_circle_and_stadium_as_ellipse() {
        let fc = parse_mermaid_flowchart("flowchart TD\n a((圆)) --> b([体育场])").unwrap();
        assert_eq!(fc.nodes[0].shape, MermaidShape::Ellipse);
        assert_eq!(fc.nodes[0].label, "圆");
        assert_eq!(fc.nodes[1].shape, MermaidShape::Ellipse);
        assert_eq!(fc.nodes[1].label, "体育场");
    }

    #[test]
    fn parses_cold_shapes_approximated_as_rectangle() {
        let fc = parse_mermaid_flowchart("flowchart TD\n a{{六边}} --> b[(柱)] --> c[[子程序]]")
            .unwrap();
        assert_eq!(fc.nodes[0].shape, MermaidShape::Rectangle);
        assert_eq!(fc.nodes[0].label, "六边");
        assert_eq!(fc.nodes[1].label, "柱");
        assert_eq!(fc.nodes[2].label, "子程序");
    }

    #[test]
    fn parses_pipe_edge_label() {
        let fc = parse_mermaid_flowchart("flowchart TD\n a{判} -->|是| b\n a -->|否| c").unwrap();
        assert_eq!(fc.edges[0].label.as_deref(), Some("是"));
        assert_eq!(fc.edges[1].label.as_deref(), Some("否"));
        assert_eq!(fc.edges[0].kind, MermaidArrow::Arrow);
    }

    #[test]
    fn parses_inline_edge_label() {
        let fc = parse_mermaid_flowchart("flowchart TD\n a -- 是 --> b").unwrap();
        assert_eq!(fc.edges[0].label.as_deref(), Some("是"));
        assert_eq!(fc.edges[0].kind, MermaidArrow::Arrow);
        // 虚线仅支持 pipe 标签，inline `-. 虚 .->` 不支持 → 报错。
        assert!(parse_mermaid_flowchart("flowchart TD\n c -. 虚 .-> d").is_err());
    }

    #[test]
    fn parses_arrow_families() {
        let fc = parse_mermaid_flowchart(
            "flowchart TD\n a --- b\n a -.-> c\n a ==> d\n a <--> e\n a -.- f",
        )
        .unwrap();
        let kinds: Vec<_> = fc.edges.iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                MermaidArrow::Open,
                MermaidArrow::Dotted,
                MermaidArrow::Thick,
                MermaidArrow::Double,
                MermaidArrow::DottedOpen,
            ]
        );
    }

    #[test]
    fn parses_amp_branch_and_cross_product() {
        let fc = parse_mermaid_flowchart("flowchart TD\n a --> b & c").unwrap();
        assert_eq!(fc.edges.len(), 2);
        assert!(fc.edges.iter().all(|e| e.from == 0));
        let mut tos: Vec<_> = fc.edges.iter().map(|e| e.to).collect();
        tos.sort_unstable();
        assert_eq!(tos, vec![1, 2]);

        // 交叉积：a & b --> c & d = a→c, a→d, b→c, b→d
        let fc2 = parse_mermaid_flowchart("flowchart TD\n a & b --> c & d").unwrap();
        assert_eq!(fc2.edges.len(), 4);
        assert!(fc2.edges.iter().all(|e| e.kind == MermaidArrow::Arrow));
    }

    #[test]
    fn parses_quoted_label_with_spaces() {
        let fc =
            parse_mermaid_flowchart("flowchart TD\n a[\"含 空格 标题\"] --> b['单引号']").unwrap();
        assert_eq!(fc.nodes[0].label, "含 空格 标题");
        assert_eq!(fc.nodes[1].label, "单引号");
    }

    #[test]
    fn bare_reference_creates_default_node() {
        let fc = parse_mermaid_flowchart("flowchart TD\n a --> b").unwrap();
        assert_eq!(fc.nodes.len(), 2);
        assert_eq!(fc.nodes[1].label, "b");
        assert_eq!(fc.nodes[1].shape, MermaidShape::Rectangle);
    }

    #[test]
    fn skips_comments_and_trailing_semicolon() {
        let fc =
            parse_mermaid_flowchart("%% 注释\n\nflowchart TD\n a --> b %% 行内注释\n c --> d; \n")
                .unwrap();
        assert_eq!(fc.nodes.len(), 4);
    }

    #[test]
    fn rejects_missing_header_and_bad_syntax() {
        assert!(parse_mermaid_flowchart("a --> b").is_err());
        assert!(parse_mermaid_flowchart("flowchart TD\n subgraph X\n a --> b\n end").is_err());
        assert!(parse_mermaid_flowchart("flowchart TD\n @@@").is_err());
        // 错误消息带行号
        let err = parse_mermaid_flowchart("flowchart TD\n subgraph X\n a --> b\n end").unwrap_err();
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
