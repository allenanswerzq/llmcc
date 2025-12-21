//! Block graph snapshot capture and rendering.

use super::{Snapshot, SnapshotContext};
use llmcc_core::context::CompileUnit;
use llmcc_core::graph_builder::BlockId;
use std::fmt::Write as _;

/// Snapshot of the block graph structure.
#[derive(Clone)]
pub struct BlockGraphSnapshot {
    /// S-expression rendering of the block tree.
    content: String,
}

impl Snapshot for BlockGraphSnapshot {
    fn capture(ctx: SnapshotContext<'_>) -> Self {
        let Some(project_graph) = ctx.project_graph else {
            return Self {
                content: "none\n".to_string(),
            };
        };

        let mut units: Vec<_> = project_graph.units().iter().collect();
        if units.is_empty() {
            return Self {
                content: "none\n".to_string(),
            };
        }

        units.sort_by_key(|unit| unit.unit_index());

        let mut sections = Vec::new();
        for unit_graph in units {
            let unit = ctx.cc.compile_unit(unit_graph.unit_index());
            let mut buf = String::new();
            render_node(unit_graph.root(), unit, 0, &mut buf);
            sections.push(buf.trim_end().to_string());
        }

        let content = if sections.is_empty() {
            "none\n".to_string()
        } else {
            let mut joined = sections.join("\n\n");
            joined.push('\n');
            joined
        };

        Self { content }
    }

    fn render(&self) -> String {
        self.content.clone()
    }

    fn normalize(text: &str) -> String {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return String::new();
        }

        match parse_sexpr(trimmed) {
            Ok(exprs) => exprs
                .into_iter()
                .map(|expr| format_sexpr(&expr))
                .collect::<Vec<_>>()
                .join("\n"),
            Err(_) => trimmed.to_string(),
        }
    }
}

fn render_node(block_id: BlockId, unit: CompileUnit<'_>, depth: usize, buf: &mut String) {
    let block = unit.bb(block_id);
    let indent = "    ".repeat(depth);
    let kind = block.kind().to_string();
    let _ = write!(buf, "{}({}:{}", indent, kind, block_id.as_u32());

    if let Some(name) = block
        .base()
        .and_then(|base| base.opt_get_name())
        .filter(|name| !name.is_empty())
    {
        let _ = write!(buf, " {}", name);
    }

    let children = block.children();
    if children.is_empty() {
        buf.push_str(")\n");
        return;
    }

    buf.push('\n');
    for &child_id in children {
        render_node(child_id, unit, depth + 1, buf);
    }
    buf.push_str(&indent);
    buf.push_str(")\n");
}

// S-expression parsing for normalization

#[derive(Debug, Clone)]
enum SExpr {
    Atom(String),
    List(Vec<SExpr>),
}

fn parse_sexpr(input: &str) -> Result<Vec<SExpr>, ()> {
    let tokens = tokenize(input);
    let mut idx = 0;
    let mut exprs = Vec::new();
    while idx < tokens.len() {
        exprs.push(parse_expr(&tokens, &mut idx)?);
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

fn parse_expr(tokens: &[String], idx: &mut usize) -> Result<SExpr, ()> {
    if *idx >= tokens.len() {
        return Err(());
    }
    let token = tokens[*idx].clone();
    *idx += 1;
    match token.as_str() {
        "(" => {
            let mut items = Vec::new();
            while *idx < tokens.len() && tokens[*idx] != ")" {
                items.push(parse_expr(tokens, idx)?);
            }
            if *idx >= tokens.len() || tokens[*idx] != ")" {
                return Err(());
            }
            *idx += 1;
            Ok(SExpr::List(items))
        }
        ")" => Err(()),
        literal => Ok(SExpr::Atom(literal.to_string())),
    }
}

fn format_sexpr(expr: &SExpr) -> String {
    match expr {
        SExpr::Atom(atom) => atom.clone(),
        SExpr::List(items) => {
            let parts: Vec<String> = items.iter().map(format_sexpr).collect();
            format!("({})", parts.join(" "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_sexpr() {
        let input = "(Module:1\n    (Fn:2 main)\n)";
        let normalized = BlockGraphSnapshot::normalize(input);
        assert_eq!(normalized, "(Module:1 (Fn:2 main))");
    }
}
