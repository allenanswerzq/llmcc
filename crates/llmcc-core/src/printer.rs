use crate::context::CompileUnit;
use crate::graph_builder::{BasicBlock, BlockId};
use crate::ir::{HirId, HirNode};
use tree_sitter::Node;

const SNIPPET_COL: usize = 60;
const TRUNCATE_COL: usize = 60;

#[derive(Debug, Clone)]
struct RenderNode {
    label: String,
    line_info: Option<String>,
    snippet: Option<String>,
    children: Vec<RenderNode>,
}

impl RenderNode {
    fn new(
        label: String,
        line_info: Option<String>,
        snippet: Option<String>,
        children: Vec<RenderNode>,
    ) -> Self {
        Self {
            label,
            line_info,
            snippet,
            children,
        }
    }
}

pub fn render_llmcc_ir<'tcx>(root: HirId, unit: CompileUnit<'tcx>) -> (String, String) {
    let hir_root = unit.hir_node(root);
    let ast_render = build_ast_render(hir_root.inner_ts_node(), unit);
    let hir_render = build_hir_render(&hir_root, unit);
    let ast = render_lines(&ast_render);
    let hir = render_lines(&hir_render);
    (ast, hir)
}

pub fn print_llmcc_ir<'tcx>(unit: CompileUnit<'tcx>) {
    let root = unit.file_start_hir_id().unwrap();
    let (ast, hir) = render_llmcc_ir(root, unit);
    println!("{}\n", ast);
    println!("{}\n", hir);
}

pub fn render_llmcc_graph<'tcx>(root: BlockId, unit: CompileUnit<'tcx>) -> String {
    let block = unit.bb(root);
    let render = build_block_render(&block, unit);
    render_lines(&render)
}

pub fn print_llmcc_graph<'tcx>(root: BlockId, unit: CompileUnit<'tcx>) {
    let graph = render_llmcc_graph(root, unit);
    println!("{}\n", graph);
}

fn build_ast_render<'tcx>(node: Node<'tcx>, unit: CompileUnit<'tcx>) -> RenderNode {
    let kind = node.kind();
    let kind_id = node.kind_id();
    let label = match field_info(node) {
        Some((name, field_id)) => format!("({name}_{field_id}):{kind} [{kind_id}]"),
        None => format!("{kind} [{kind_id}]"),
    };

    // Get line information from byte positions
    let start_line = get_line_from_byte(&unit, node.start_byte());
    let end_line = get_line_from_byte(&unit, node.end_byte());
    let line_info = Some(format!("[{}-{}]", start_line, end_line));

    let snippet = snippet_from_ctx(&unit, node.start_byte(), node.end_byte());

    let mut cursor = node.walk();
    let children = node
        .children(&mut cursor)
        .map(|child| build_ast_render(child, unit))
        .collect();

    RenderNode::new(label, line_info, snippet, children)
}

fn build_hir_render<'tcx>(node: &HirNode<'tcx>, unit: CompileUnit<'tcx>) -> RenderNode {
    let label = node.format_node(unit);

    // Get line information from byte positions
    let start_line = get_line_from_byte(&unit, node.start_byte());
    let end_line = get_line_from_byte(&unit, node.end_byte());
    let line_info = Some(format!("[{}-{}]", start_line, end_line));

    let snippet = snippet_from_ctx(&unit, node.start_byte(), node.end_byte());
    let children = node
        .children()
        .iter()
        .map(|id| {
            let child = unit.hir_node(*id);
            build_hir_render(&child, unit)
        })
        .collect();
    RenderNode::new(label, line_info, snippet, children)
}

fn build_block_render<'tcx>(block: &BasicBlock<'tcx>, unit: CompileUnit<'tcx>) -> RenderNode {
    let label = block.format_block(unit);

    // Get line information from the block's node
    let line_info = block.opt_node().map(|node| {
        let start_line = get_line_from_byte(&unit, node.start_byte());
        let end_line = get_line_from_byte(&unit, node.end_byte());
        format!("[{}-{}]", start_line, end_line)
    });

    let snippet = block
        .opt_node()
        .and_then(|n| snippet_from_ctx(&unit, n.start_byte(), n.end_byte()));
    let children = block
        .children()
        .iter()
        .map(|id| {
            let child = unit.bb(*id);
            build_block_render(&child, unit)
        })
        .collect();
    RenderNode::new(label, line_info, snippet, children)
}

fn render_lines(node: &RenderNode) -> String {
    let mut lines = Vec::new();
    render_node(node, 0, &mut lines);
    lines.join("\n")
}

fn render_node(node: &RenderNode, depth: usize, out: &mut Vec<String>) {
    let indent = "  ".repeat(depth);
    let mut line = format!("{}({}", indent, node.label);

    // Add line information if available
    if let Some(line_info) = &node.line_info {
        line.push_str(&format!(" {}", line_info));
    }

    if let Some(snippet) = &node.snippet {
        let padded = pad_snippet(&line, snippet);
        line.push_str(&padded);
    }

    if node.children.is_empty() {
        line.push(')');
        out.push(line);
    } else {
        out.push(line);
        for child in &node.children {
            render_node(child, depth + 1, out);
        }
        out.push(format!("{})", indent));
    }
}

fn safe_truncate(s: &mut String, max_len: usize) {
    if s.len() > max_len {
        let mut new_len = max_len;
        while !s.is_char_boundary(new_len) {
            new_len -= 1;
        }
        s.truncate(new_len);
    }
}

fn pad_snippet(line: &str, snippet: &str) -> String {
    let mut snippet = snippet.trim().replace('\n', " ");
    if snippet.len() > TRUNCATE_COL {
        safe_truncate(&mut snippet, TRUNCATE_COL);
        snippet.push_str("...");
    }

    if snippet.is_empty() {
        return String::new();
    }

    let padding = SNIPPET_COL.saturating_sub(line.len());
    format!("{}|{}|", " ".repeat(padding), snippet)
}

fn snippet_from_ctx(unit: &CompileUnit<'_>, start: usize, end: usize) -> Option<String> {
    unit.file()
        .opt_get_text(start, end)
        .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| !s.is_empty())
}

/// Get line number from byte position
fn get_line_from_byte(unit: &CompileUnit<'_>, byte_pos: usize) -> usize {
    let content = unit.file().content();
    let text = String::from_utf8_lossy(&content[..byte_pos.min(content.len())]);
    text.lines().count()
}

fn field_info(node: Node<'_>) -> Option<(String, u16)> {
    let parent = node.parent()?;
    let mut cursor = parent.walk();
    if !cursor.goto_first_child() {
        return None;
    }
    loop {
        if cursor.node().id() == node.id() {
            let name = cursor.field_name()?.to_string();
            let id = cursor.field_id()?.get();
            return Some((name, id));
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn node(label: &str, snippet: Option<&str>, children: Vec<RenderNode>) -> RenderNode {
        RenderNode::new(
            label.to_string(),
            None,
            snippet.map(ToOwned::to_owned),
            children,
        )
    }

    #[test]
    fn render_tree_formats_nested_structure() {
        let tree = node(
            "root",
            Some("snippet text"),
            vec![
                node("child1", None, vec![]),
                node(
                    "child2",
                    Some("long snippet for child two"),
                    vec![node("grandchild", None, vec![])],
                ),
            ],
        );

        let rendered = render_lines(&tree);
        let lines: Vec<&str> = rendered.lines().collect();

        assert!(lines[0].starts_with("(root"));
        assert!(lines[0].contains("|snippet text|"));
        assert_eq!(lines[1].trim_start(), "(child1)");
        assert!(lines[2].trim_start().starts_with("(child2"));
        assert!(lines[2].contains("|long snippet for child two|"));
        assert_eq!(lines[3].trim_start(), "(grandchild)");
        assert_eq!(lines[4].trim(), ")");
        assert_eq!(lines[5], ")");
    }

    #[test]
    fn render_tree_truncates_long_snippets() {
        let long_snippet = "a very long snippet that should be truncated for readability because it exceeds the maximum column width specified by the printer logic";
        let tree = node("root", Some(long_snippet), vec![]);
        let rendered = render_lines(&tree);
        assert!(rendered.contains("a very long snippet"));
        assert!(rendered.contains("...|"));
        assert!(rendered.ends_with(")"));
    }
}
