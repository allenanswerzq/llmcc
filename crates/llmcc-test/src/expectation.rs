use std::fmt::Write as _;

use similar::TextDiff;

pub(crate) fn format_expectation_diff(kind: &str, expected: &str, actual: &str) -> String {
    let diff = TextDiff::from_lines(expected, actual);
    let mut buf = String::new();
    let _ = writeln!(buf, "Expectation '{kind}' mismatch:");
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            similar::ChangeTag::Delete => "-",
            similar::ChangeTag::Insert => "+",
            similar::ChangeTag::Equal => " ",
        };
        let _ = write!(buf, "{sign}{change}");
    }
    buf
}

pub(crate) fn normalize(kind: &str, text: &str, temp_dir_path: Option<&str>) -> String {
    let canonical = text
        .replace("\r\n", "\n")
        .trim_end_matches('\n')
        .to_string();

    let canonical = if let Some(tmp_path) = temp_dir_path {
        let mut result = canonical.replace(tmp_path, "$TMP");
        if let Some(dir_name) = std::path::Path::new(tmp_path)
            .file_name()
            .and_then(|s| s.to_str())
        {
            result = result.replace(dir_name, "$TMP");
        }
        result
    } else {
        canonical
    };

    match kind {
        "symbols" | "blocks" | "symbol-types" => normalize_symbols(&canonical),
        "symbol-deps" | "block-deps" => normalize_symbol_deps(&canonical),
        "block-relations" => normalize_block_relations(&canonical),
        "dep-graph" | "arch-graph" | "arch-graph-depth-0" | "arch-graph-depth-1"
        | "arch-graph-depth-2" | "arch-graph-depth-3" => normalize_graph(&canonical),
        "block-graph" => normalize_block_graph(&canonical),
        _ => canonical,
    }
}

pub(crate) fn ensure_trailing_newline(mut text: String) -> String {
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn normalize_symbols(text: &str) -> String {
    let mut rows: Vec<(usize, u32, String)> = text
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }

            let parts: Vec<_> = line.split('|').map(|part| part.trim()).collect();
            if parts.is_empty() {
                return None;
            }

            let label = parts[0];
            let (unit, id) = parse_unit_and_id(label);
            let kind = parts.get(1).copied().unwrap_or("");
            let name = parts.get(2).copied().unwrap_or("");
            let global = parts.get(3).copied().unwrap_or("");

            let canonical = format!("{label} | {kind} | {name} | {global}");
            Some((unit, id, canonical.trim_end().to_string()))
        })
        .collect();

    rows.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });

    rows.into_iter()
        .map(|(_, _, row)| row)
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_symbol_deps(text: &str) -> String {
    let mut rows: Vec<_> = text
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || is_empty_relation(trimmed) {
                return None;
            }
            Some(trimmed.to_string())
        })
        .collect();
    rows.sort();
    rows.join("\n")
}

fn normalize_block_relations(text: &str) -> String {
    let mut lines: Vec<String> = text
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();

    lines.sort();
    lines.join("\n")
}

fn normalize_block_graph(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    match parse_sexpr(trimmed) {
        Ok(exprs) => exprs
            .into_iter()
            .map(|expr| format_sexpr(&expr))
            .collect::<Vec<_>>()
            .join("\n\n"),
        Err(_) => trimmed.to_string(),
    }
}

fn is_empty_relation(line: &str) -> bool {
    if let Some((_, rhs)) = line.split_once("->")
        && rhs.trim() == "[]"
    {
        return true;
    }
    if let Some((_, rhs)) = line.split_once("<-")
        && rhs.trim() == "[]"
    {
        return true;
    }
    false
}

fn normalize_graph(text: &str) -> String {
    let mut lines: Vec<&str> = text
        .trim()
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return false;
            }
            if trimmed.starts_with("//")
                || trimmed.starts_with("rankdir=")
                || trimmed.starts_with("ranksep=")
                || trimmed.starts_with("nodesep=")
                || trimmed.starts_with("splines=")
                || trimmed.starts_with("compound=")
                || trimmed.starts_with("concentrate=")
                || trimmed.starts_with("fontsize=")
                || trimmed.starts_with("fontname=")
                || trimmed.starts_with("labelloc=")
                || trimmed.starts_with("node [")
                || trimmed.starts_with("edge [")
                || trimmed.starts_with("style=")
                || trimmed.starts_with("color=")
                || trimmed.starts_with("bgcolor=")
                || trimmed.starts_with("label=\"")
            {
                return false;
            }
            true
        })
        .collect();

    let edge_re = regex::Regex::new(r"^\s*n\d+\s*->\s*n\d+").unwrap();
    let mut edge_start = None;
    let mut edge_end = None;

    for (index, line) in lines.iter().enumerate() {
        if edge_re.is_match(line) {
            if edge_start.is_none() {
                edge_start = Some(index);
            }
            edge_end = Some(index);
        }
    }

    if let (Some(start), Some(end)) = (edge_start, edge_end) {
        lines[start..=end].sort();
    }

    lines.join("\n")
}

fn parse_unit_and_id(token: &str) -> (usize, u32) {
    if let Some(stripped) = token.strip_prefix('u')
        && let Some((unit_str, id_str)) = stripped.split_once(':')
        && let (Ok(unit), Ok(id)) = (unit_str.parse::<usize>(), id_str.parse::<u32>())
    {
        return (unit, id);
    }

    (usize::MAX, u32::MAX)
}

#[derive(Debug, Clone)]
enum SExpr {
    Atom(String),
    List(Vec<SExpr>),
}

fn parse_sexpr(input: &str) -> std::result::Result<Vec<SExpr>, ()> {
    let tokens = tokenize(input);
    let mut index = 0;
    let mut exprs = Vec::new();
    while index < tokens.len() {
        exprs.push(parse_expr(&tokens, &mut index)?);
    }
    Ok(exprs)
}

fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '(' | ')' => {
                if !current.trim().is_empty() {
                    tokens.push(current.trim().to_string());
                }
                current.clear();
                tokens.push(ch.to_string());
            }
            '"' => {
                if !current.trim().is_empty() {
                    tokens.push(current.trim().to_string());
                }
                current.clear();
                let mut literal = String::new();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == '"' {
                        break;
                    }
                    literal.push(next);
                }
                tokens.push(literal);
            }
            _ if ch.is_whitespace() => {
                if !current.trim().is_empty() {
                    tokens.push(current.trim().to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        tokens.push(current.trim().to_string());
    }
    tokens
}

fn parse_expr(tokens: &[String], index: &mut usize) -> std::result::Result<SExpr, ()> {
    if *index >= tokens.len() {
        return Err(());
    }
    let token = tokens[*index].clone();
    *index += 1;
    match token.as_str() {
        "(" => {
            let mut items = Vec::new();
            while *index < tokens.len() && tokens[*index] != ")" {
                items.push(parse_expr(tokens, index)?);
            }
            if *index >= tokens.len() || tokens[*index] != ")" {
                return Err(());
            }
            *index += 1;
            Ok(SExpr::List(items))
        }
        ")" => Err(()),
        literal => Ok(SExpr::Atom(literal.to_string())),
    }
}

fn format_sexpr(expr: &SExpr) -> String {
    format_sexpr_indented(expr, 0)
}

fn format_sexpr_indented(expr: &SExpr, depth: usize) -> String {
    match expr {
        SExpr::Atom(atom) => atom.clone(),
        SExpr::List(items) => {
            if items.is_empty() {
                return "()".to_string();
            }

            let head_parts: Vec<String> = items
                .iter()
                .take_while(|item| matches!(item, SExpr::Atom(_)))
                .map(format_sexpr)
                .collect();
            let head = head_parts.join(" ");
            let children: Vec<&SExpr> = items.iter().skip(head_parts.len()).collect();

            if children.is_empty() {
                format!("({head})")
            } else {
                let indent = "  ".repeat(depth);
                let child_indent = "  ".repeat(depth + 1);
                let mut buf = format!("({head}\n");
                for child in children {
                    buf.push_str(&child_indent);
                    buf.push_str(&format_sexpr_indented(child, depth + 1));
                    buf.push('\n');
                }
                buf.push_str(&indent);
                buf.push(')');
                buf
            }
        }
    }
}
