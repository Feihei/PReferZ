//! 两列数据解析（plan #8）：把剪贴板文本识别为「标签列 + 数值列」，
//! 供粘贴成柱状/折线图（`ItemKind::Chart`）。纯函数、无副作用（core L1 惯例）。

/// 解析 2 列 TSV/CSV 数据。规则：
/// - 按行拆分（容忍 `\r\n`）；忽略首尾空行；有效行 ≥ 2；
/// - 分隔符：任一行含 Tab 用 Tab（Excel/表格软件复制即 TSV），否则用逗号；
/// - 每行必须恰好 2 个单元格（trim 后），否则整段拒绝；
/// - 第二列可解析为 `f32` 的行是数据行；仅**第一行**允许不解析（表头，
///   如「月份<Tab>销量」）且其余全为数据行时丢弃表头；其余任何非数值行 → 拒绝；
/// - 返回 `(标签, 数值)`（一一对应，数据行 ≥ 2）。
pub fn parse_two_column_data(text: &str) -> Option<(Vec<String>, Vec<f32>)> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let mut start = 0;
    let mut end = lines.len();
    while start < end && lines[start].is_empty() {
        start += 1;
    }
    while end > start && lines[end - 1].is_empty() {
        end -= 1;
    }
    let lines = &lines[start..end];
    if lines.len() < 2 {
        return None;
    }
    let delim = if lines.iter().any(|l| l.contains('\t')) {
        '\t'
    } else {
        ','
    };
    let mut labels: Vec<String> = Vec::with_capacity(lines.len());
    let mut values: Vec<f32> = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        let mut cells = line.split(delim);
        let a = cells.next()?;
        let b = cells.next()?;
        if cells.next().is_some() {
            return None; // 超过 2 列
        }
        match b.parse::<f32>() {
            Ok(v) => {
                labels.push(a.trim().to_string());
                values.push(v);
            }
            Err(_) => {
                // 仅第一行可作表头丢弃；数据行不足 2 时同样在此被拦下。
                if i == 0 && values.is_empty() {
                    labels.clear();
                    continue;
                }
                return None;
            }
        }
    }
    if values.len() < 2 {
        return None;
    }
    Some((labels, values))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tsv_from_spreadsheet() {
        // Excel 复制典型形态：\r\n 行尾 + Tab 分隔 + 末尾换行
        let (labels, values) = parse_two_column_data("一月\t12\r\n二月\t34\r\n").unwrap();
        assert_eq!(labels, vec!["一月", "二月"]);
        assert_eq!(values, vec![12.0, 34.0]);
    }

    #[test]
    fn parses_csv_and_skips_header() {
        let (labels, values) = parse_two_column_data("month,sales\nJan,10\nFeb,20").unwrap();
        assert_eq!(labels, vec!["Jan", "Feb"]);
        assert_eq!(values, vec![10.0, 20.0]);
    }

    #[test]
    fn numeric_labels_allowed_without_header() {
        let (labels, values) = parse_two_column_data("2023,100\n2024,200").unwrap();
        assert_eq!(labels, vec!["2023", "2024"]);
        assert_eq!(values, vec![100.0, 200.0]);
    }

    #[test]
    fn rejects_three_columns() {
        assert!(parse_two_column_data("a\t1\tx\nb\t2\ty").is_none());
    }

    #[test]
    fn rejects_non_numeric_mid_row() {
        // 第二行数值列非法且不是第一行 → 拒绝（不做表头容忍）
        assert!(parse_two_column_data("a\t1\nb\tx\nc\t3").is_none());
        assert!(parse_two_column_data("a\t1\nb\tten").is_none());
    }

    #[test]
    fn rejects_single_data_row() {
        assert!(parse_two_column_data("a\t1").is_none());
        // 表头 + 单数据行也不够
        assert!(parse_two_column_data("h\tv\na\t1").is_none());
    }

    #[test]
    fn rejects_empty_and_garbage() {
        assert!(parse_two_column_data("").is_none());
        assert!(parse_two_column_data("hello world").is_none());
    }

    #[test]
    fn negative_values_accepted() {
        let (_, values) = parse_two_column_data("a,-1.5\nb,2").unwrap();
        assert_eq!(values, vec![-1.5, 2.0]);
    }
}
