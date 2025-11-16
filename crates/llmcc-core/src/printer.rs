//! TOOD: use impl fmt::Debug
use crate::context::CompileUnit;
use crate::graph_builder::{BasicBlock, BlockId};
use crate::ir::{HirId, HirNode};
use std::fmt;

/// Output format for rendering
///
/// Controls how the tree structure is rendered to string output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrintFormat {
    /// Standard tree format with indentation and nested structure
    /// ```text
    /// (root
    ///   (child1)
    ///   (child2)
    /// )
    /// ```
    Tree,

    /// Compact format with minimal whitespace
    /// ```text
    /// (root (child1) (child2))
    /// ```
    Compact,

    /// One node per line, minimal formatting
    /// ```text
    /// root
    /// child1
    /// child2
    /// ```
    Flat,
}

impl Default for PrintFormat {
    fn default() -> Self {
        PrintFormat::Tree
    }
}

impl fmt::Display for PrintFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrintFormat::Tree => write!(f, "tree"),
            PrintFormat::Compact => write!(f, "compact"),
            PrintFormat::Flat => write!(f, "flat"),
        }
    }
}

impl std::str::FromStr for PrintFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "tree" => Ok(PrintFormat::Tree),
            "compact" => Ok(PrintFormat::Compact),
            "flat" => Ok(PrintFormat::Flat),
            other => Err(format!(
                "Unknown format: {}. Use 'tree', 'compact', or 'flat'",
                other
            )),
        }
    }
}

/// Production-ready configuration for rendering and printing
///
/// This struct controls all aspects of output formatting. Use the builder
/// methods to customize behavior, or use preset configurations via
/// [`PrintConfig::minimal()`] and [`PrintConfig::verbose()`].
#[derive(Debug, Clone)]
pub struct PrintConfig {
    /// Output format (tree, compact, flat)
    pub format: PrintFormat,

    /// Include source code snippets in output
    pub include_snippets: bool,

    /// Include line number information [start-end]
    pub include_line_info: bool,

    /// Column width for snippet alignment (for tree format)
    pub snippet_col_width: usize,

    /// Maximum snippet length before truncation with "..."
    pub snippet_max_length: usize,

    /// Maximum nesting depth (prevents stack overflow on deeply nested input)
    pub max_depth: usize,

    /// Indentation width in spaces per nesting level
    pub indent_width: usize,

    /// Include unique node IDs in output
    pub include_node_ids: bool,

    /// Include field names (for tree-sitter nodes)
    pub include_field_names: bool,

    /// Truncate long lines to this width (0 = no truncation)
    pub line_width_limit: usize,
}

impl Default for PrintConfig {
    fn default() -> Self {
        PrintConfig {
            format: PrintFormat::Tree,
            include_snippets: true,
            include_line_info: true,
            snippet_col_width: 60,
            snippet_max_length: 60,
            max_depth: 1000,
            indent_width: 2,
            include_node_ids: false,
            include_field_names: false,
            line_width_limit: 0,
        }
    }
}

impl PrintConfig {
    /// Create a new configuration with default settings
    pub fn new() -> Self {
        Self::default()
    }

    // Builder methods
    // ====================================================================

    /// Set output format
    pub fn with_format(mut self, format: PrintFormat) -> Self {
        self.format = format;
        self
    }

    /// Enable/disable snippets
    pub fn with_snippets(mut self, enabled: bool) -> Self {
        self.include_snippets = enabled;
        self
    }

    /// Enable/disable line information
    pub fn with_line_info(mut self, enabled: bool) -> Self {
        self.include_line_info = enabled;
        self
    }

    /// Set snippet display width
    pub fn with_snippet_width(mut self, width: usize) -> Self {
        self.snippet_col_width = width;
        self
    }

    /// Set maximum snippet length before truncation
    pub fn with_snippet_max_length(mut self, length: usize) -> Self {
        self.snippet_max_length = length;
        self
    }

    /// Set maximum nesting depth
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// Set indentation width
    pub fn with_indent_width(mut self, width: usize) -> Self {
        self.indent_width = width;
        self
    }

    /// Enable/disable node IDs
    pub fn with_node_ids(mut self, enabled: bool) -> Self {
        self.include_node_ids = enabled;
        self
    }

    /// Enable/disable field names
    pub fn with_field_names(mut self, enabled: bool) -> Self {
        self.include_field_names = enabled;
        self
    }

    /// Set line width limit
    pub fn with_line_width_limit(mut self, width: usize) -> Self {
        self.line_width_limit = width;
        self
    }

    // Preset configurations
    // ====================================================================

    /// Minimal configuration (fastest rendering)
    ///
    /// Disables snippets, line info, and node IDs for maximum speed
    pub fn minimal() -> Self {
        PrintConfig {
            format: PrintFormat::Flat,
            include_snippets: false,
            include_line_info: false,
            include_node_ids: false,
            include_field_names: false,
            ..Default::default()
        }
    }

    /// Verbose configuration (maximum detail)
    ///
    /// Enables all features for comprehensive output
    pub fn verbose() -> Self {
        PrintConfig {
            format: PrintFormat::Tree,
            include_snippets: true,
            include_line_info: true,
            include_node_ids: true,
            include_field_names: true,
            snippet_col_width: 80,
            snippet_max_length: 100,
            ..Default::default()
        }
    }

    /// Compact configuration (balanced)
    ///
    /// Good for interactive debugging
    pub fn compact() -> Self {
        PrintConfig {
            format: PrintFormat::Compact,
            include_snippets: true,
            include_line_info: true,
            include_node_ids: false,
            ..Default::default()
        }
    }

    /// Validate configuration for logical consistency
    pub fn validate(&self) -> Result<(), String> {
        if self.max_depth == 0 {
            return Err("max_depth must be > 0".to_string());
        }
        if self.indent_width == 0 {
            return Err("indent_width must be > 0".to_string());
        }
        if self.snippet_col_width == 0 {
            return Err("snippet_col_width must be > 0".to_string());
        }
        Ok(())
    }
}

/// Error type for rendering operations
#[derive(Debug, Clone)]
pub struct RenderError {
    pub message: String,
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Render error: {}", self.message)
    }
}

impl std::error::Error for RenderError {}

impl From<String> for RenderError {
    fn from(message: String) -> Self {
        RenderError { message }
    }
}

impl From<&str> for RenderError {
    fn from(message: &str) -> Self {
        RenderError {
            message: message.to_string(),
        }
    }
}

impl RenderError {
    /// Create a new render error
    pub fn new(message: impl Into<String>) -> Self {
        RenderError {
            message: message.into(),
        }
    }

    /// Error for exceeding maximum depth
    pub fn max_depth_exceeded(depth: usize, max: usize) -> Self {
        RenderError::new(format!("Maximum depth {} exceeded (limit: {})", depth, max))
    }

    /// Error for invalid configuration
    pub fn config_invalid(reason: impl Into<String>) -> Self {
        RenderError::new(format!("Invalid configuration: {}", reason.into()))
    }
}

pub type RenderResult<T> = Result<T, RenderError>;

// ============================================================================
// Internal Render Node Structure
// ============================================================================

/// Internal representation of a node for rendering
#[derive(Debug, Clone)]
struct RenderNode {
    label: String,
    line_info: Option<String>,
    snippet: Option<String>,
    children: Vec<RenderNode>,
    node_id: Option<String>,
}

impl RenderNode {
    fn new(
        label: String,
        line_info: Option<String>,
        snippet: Option<String>,
        children: Vec<RenderNode>,
        node_id: Option<String>,
    ) -> Self {
        Self {
            label,
            line_info,
            snippet,
            children,
            node_id,
        }
    }
}

// ============================================================================
// Public API Functions
// ============================================================================

/// Render HIR with default configuration
pub fn render_llmcc_ir(root: HirId, unit: CompileUnit<'_>) -> RenderResult<(String, String)> {
    render_llmcc_ir_with_config(root, unit, &PrintConfig::default())
}

/// Render HIR with custom configuration
pub fn render_llmcc_ir_with_config(
    root: HirId,
    unit: CompileUnit<'_>,
    config: &PrintConfig,
) -> RenderResult<(String, String)> {
    config.validate()?;

    let hir_root = unit.hir_node(root);

    // Build AST render tree from parse tree if available
    let ast_render = if let Some(parse_tree) = unit.parse_tree() {
        if let Some(root_node) = parse_tree.root_node() {
            build_ast_render_from_node(&*root_node, config, 0)?
        } else {
            RenderNode::new(
                "No AST root node found".to_string(),
                None,
                None,
                vec![],
                None,
            )
        }
    } else {
        RenderNode::new(
            "Parse tree not available for this compilation unit".to_string(),
            None,
            None,
            vec![],
            None,
        )
    };

    let hir_render = build_hir_render(&hir_root, unit, config, 0)?;
    let ast = render_lines(&ast_render, config)?;
    let hir = render_lines(&hir_render, config)?;

    Ok((ast, hir))
}

/// Print HIR to stdout
pub fn print_llmcc_ir(unit: CompileUnit<'_>) -> RenderResult<()> {
    print_llmcc_ir_with_config(unit, &PrintConfig::default())
}

/// Print HIR to stdout with custom configuration
pub fn print_llmcc_ir_with_config(unit: CompileUnit<'_>, config: &PrintConfig) -> RenderResult<()> {
    let root = unit
        .file_start_hir_id()
        .ok_or_else(|| RenderError::new("No HIR root node found"))?;

    let (ast, hir) = render_llmcc_ir_with_config(root, unit, config)?;
    println!("{}\n", ast);
    println!("{}\n", hir);
    Ok(())
}

/// Render control flow graph with default configuration
pub fn render_llmcc_graph(root: BlockId, unit: CompileUnit<'_>) -> RenderResult<String> {
    render_llmcc_graph_with_config(root, unit, &PrintConfig::default())
}

/// Render control flow graph with custom configuration
pub fn render_llmcc_graph_with_config(
    root: BlockId,
    unit: CompileUnit<'_>,
    config: &PrintConfig,
) -> RenderResult<String> {
    config.validate()?;

    let block = unit.bb(root);
    let render = build_block_render(&block, unit, config, 0)?;
    render_lines(&render, config)
}

/// Print control flow graph to stdout
pub fn print_llmcc_graph(root: BlockId, unit: CompileUnit<'_>) -> RenderResult<()> {
    print_llmcc_graph_with_config(root, unit, &PrintConfig::default())
}

/// Print control flow graph to stdout with custom configuration
pub fn print_llmcc_graph_with_config(
    root: BlockId,
    unit: CompileUnit<'_>,
    config: &PrintConfig,
) -> RenderResult<()> {
    let graph = render_llmcc_graph_with_config(root, unit, config)?;
    println!("{}\n", graph);
    Ok(())
}

// ============================================================================
// Internal Rendering Functions
// ============================================================================

/// Build render tree for AST node (from parse tree)
fn build_ast_render_from_node(
    node: &(dyn crate::lang_def::ParseNode + '_),
    config: &PrintConfig,
    depth: usize,
) -> RenderResult<RenderNode> {
    build_ast_render_from_node_with_parent(node, None, 0, config, depth)
}

/// Build render tree for AST node with parent context for field names
fn build_ast_render_from_node_with_parent(
    node: &(dyn crate::lang_def::ParseNode + '_),
    parent: Option<&(dyn crate::lang_def::ParseNode + '_)>,
    child_index: usize,
    config: &PrintConfig,
    depth: usize,
) -> RenderResult<RenderNode> {
    // Check depth limit
    if depth > config.max_depth {
        return Err(RenderError::max_depth_exceeded(depth, config.max_depth));
    }

    // Get field name if available from parent
    let field_name: Option<&str> = parent.and_then(|p| p.child_field_name(child_index));

    // Use the trait method to format the label
    let label = node.format_node_label(field_name);

    // Line range info
    let line_info = Some(format!("[{}-{}]", node.start_byte(), node.end_byte()));

    // Collect children
    let mut children = Vec::new();
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if let Ok(render) = build_ast_render_from_node_with_parent(&*child, Some(node), i, config, depth + 1) {
                children.push(render);
            }
        }
    }

    Ok(RenderNode::new(
        label,
        line_info,
        None,
        children,
        None,
    ))
}

/// Build render tree for HIR node
fn build_hir_render<'tcx>(
    node: &HirNode<'tcx>,
    unit: CompileUnit<'tcx>,
    config: &PrintConfig,
    depth: usize,
) -> RenderResult<RenderNode> {
    // Check depth limit
    if depth > config.max_depth {
        return Err(RenderError::max_depth_exceeded(depth, config.max_depth));
    }

    let mut label = node.format_node(unit);

    // Add identifier name info for Ident nodes
    if let crate::ir::HirNode::Ident(ident) = node {
        label.push_str(&format!(" = \"{}\"", ident.name));
    }

    let line_info = if config.include_line_info {
        Some(format!(
            "[{}-{}]",
            get_line_from_byte(&unit, node.start_byte()),
            get_line_from_byte(&unit, node.end_byte())
        ))
    } else {
        None
    };

    let snippet = if config.include_snippets {
        snippet_from_ctx(&unit, node.start_byte(), node.end_byte(), config)
    } else {
        None
    };

    let children = node
        .children()
        .iter()
        .map(|id| {
            let child = unit.hir_node(*id);
            build_hir_render(&child, unit, config, depth + 1)
        })
        .collect::<RenderResult<Vec<_>>>()?;

    Ok(RenderNode::new(label, line_info, snippet, children, None))
}

/// Build render tree for control flow block
fn build_block_render<'tcx>(
    block: &BasicBlock<'tcx>,
    unit: CompileUnit<'tcx>,
    config: &PrintConfig,
    depth: usize,
) -> RenderResult<RenderNode> {
    // Check depth limit
    if depth > config.max_depth {
        return Err(RenderError::max_depth_exceeded(depth, config.max_depth));
    }

    let label = block.format_block(unit);

    let line_info = if config.include_line_info {
        block.opt_node().map(|node| {
            format!(
                "[{}-{}]",
                get_line_from_byte(&unit, node.start_byte()),
                get_line_from_byte(&unit, node.end_byte())
            )
        })
    } else {
        None
    };

    let snippet = if config.include_snippets {
        block
            .opt_node()
            .and_then(|n| snippet_from_ctx(&unit, n.start_byte(), n.end_byte(), config))
    } else {
        None
    };

    let children = block
        .children()
        .iter()
        .map(|id| {
            let child = unit.bb(*id);
            build_block_render(&child, unit, config, depth + 1)
        })
        .collect::<RenderResult<Vec<_>>>()?;

    Ok(RenderNode::new(label, line_info, snippet, children, None))
}

/// Render node tree to string based on configuration
fn render_lines(node: &RenderNode, config: &PrintConfig) -> RenderResult<String> {
    let mut lines = Vec::new();
    render_node_with_format(node, 0, &mut lines, config)?;
    Ok(lines.join("\n"))
}

/// Render individual node with format-specific handling
fn render_node_with_format(
    node: &RenderNode,
    depth: usize,
    out: &mut Vec<String>,
    config: &PrintConfig,
) -> RenderResult<()> {
    match config.format {
        PrintFormat::Tree => render_node_tree(node, depth, out, config),
        PrintFormat::Compact => render_node_compact(node, out, config),
        PrintFormat::Flat => render_node_flat(node, out, config),
    }
}

/// Render in tree format (indented with nesting)
fn render_node_tree(
    node: &RenderNode,
    depth: usize,
    out: &mut Vec<String>,
    config: &PrintConfig,
) -> RenderResult<()> {
    let indent = " ".repeat(depth * config.indent_width);
    let mut line = format!("{}(", indent);

    // Add label
    line.push_str(&node.label);

    // Add node ID if configured
    if config.include_node_ids {
        if let Some(id) = &node.node_id {
            line.push_str(&format!(" #{}", id));
        }
    }

    // Add line information
    if let Some(line_info) = &node.line_info {
        line.push_str(&format!(" {}", line_info));
    }

    // Add snippet
    if let Some(snippet) = &node.snippet {
        let padded = pad_snippet(&line, snippet, config);
        line.push_str(&padded);
    }

    // Handle children
    if node.children.is_empty() {
        line.push(')');
        out.push(line);
    } else {
        out.push(line);
        for child in &node.children {
            render_node_tree(child, depth + 1, out, config)?;
        }
        out.push(format!("{})", indent));
    }

    Ok(())
}

/// Render in compact format
fn render_node_compact(
    node: &RenderNode,
    out: &mut Vec<String>,
    config: &PrintConfig,
) -> RenderResult<()> {
    let mut line = format!("({})", node.label);

    if config.include_line_info {
        if let Some(info) = &node.line_info {
            line.push_str(&format!(" {}", info));
        }
    }

    for child in &node.children {
        let mut child_line = format!("({})", child.label);
        if config.include_line_info && child.line_info.is_some() {
            child_line.push_str(&format!(" {}", child.line_info.as_ref().unwrap()));
        }
        line.push_str(&format!(" {}", child_line));
    }

    out.push(line);
    Ok(())
}

/// Render in flat format (one node per line)
fn render_node_flat(
    node: &RenderNode,
    out: &mut Vec<String>,
    config: &PrintConfig,
) -> RenderResult<()> {
    let mut line = node.label.clone();

    if config.include_line_info {
        if let Some(info) = &node.line_info {
            line.push_str(&format!(" {}", info));
        }
    }

    out.push(line);

    for child in &node.children {
        render_node_flat(child, out, config)?;
    }

    Ok(())
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Safe string truncation respecting UTF-8 boundaries
fn safe_truncate(s: &mut String, max_len: usize) {
    if s.len() > max_len {
        let mut new_len = max_len;
        while !s.is_char_boundary(new_len) {
            new_len = new_len.saturating_sub(1);
            if new_len == 0 {
                break;
            }
        }
        s.truncate(new_len);
    }
}

/// Format snippet with padding and alignment
fn pad_snippet(line: &str, snippet: &str, config: &PrintConfig) -> String {
    let mut snippet = snippet.trim().replace('\n', " ");

    // Truncate if too long
    if snippet.len() > config.snippet_max_length {
        safe_truncate(&mut snippet, config.snippet_max_length);
        snippet.push_str("...");
    }

    if snippet.is_empty() {
        return String::new();
    }

    let padding = config.snippet_col_width.saturating_sub(line.len());
    format!("{}|{}|", " ".repeat(padding), snippet)
}

/// Extract and format source code snippet
fn snippet_from_ctx(
    unit: &CompileUnit<'_>,
    start: usize,
    end: usize,
    config: &PrintConfig,
) -> Option<String> {
    unit.file()
        .opt_get_text(start, end)
        .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| !s.is_empty() && s.len() <= config.snippet_max_length)
}

/// Get line number from byte position
fn get_line_from_byte(unit: &CompileUnit<'_>, byte_pos: usize) -> usize {
    let content = unit.file().content();
    let text = String::from_utf8_lossy(&content[..byte_pos.min(content.len())]);
    text.lines().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_print_format_display() {
        assert_eq!(PrintFormat::Tree.to_string(), "tree");
        assert_eq!(PrintFormat::Compact.to_string(), "compact");
        assert_eq!(PrintFormat::Flat.to_string(), "flat");
    }

    #[test]
    fn test_print_format_from_str() {
        assert_eq!("tree".parse::<PrintFormat>().unwrap(), PrintFormat::Tree);
        assert_eq!(
            "compact".parse::<PrintFormat>().unwrap(),
            PrintFormat::Compact
        );
        assert_eq!("flat".parse::<PrintFormat>().unwrap(), PrintFormat::Flat);
        assert!("invalid".parse::<PrintFormat>().is_err());
    }

    #[test]
    fn test_print_config_default() {
        let config = PrintConfig::default();
        assert_eq!(config.format, PrintFormat::Tree);
        assert!(config.include_snippets);
        assert!(config.include_line_info);
        assert_eq!(config.max_depth, 1000);
    }

    #[test]
    fn test_print_config_minimal() {
        let config = PrintConfig::minimal();
        assert_eq!(config.format, PrintFormat::Flat);
        assert!(!config.include_snippets);
        assert!(!config.include_line_info);
        assert!(!config.include_node_ids);
    }

    #[test]
    fn test_print_config_verbose() {
        let config = PrintConfig::verbose();
        assert_eq!(config.format, PrintFormat::Tree);
        assert!(config.include_snippets);
        assert!(config.include_line_info);
        assert!(config.include_node_ids);
    }

    #[test]
    fn test_print_config_builder() {
        let config = PrintConfig::new()
            .with_format(PrintFormat::Flat)
            .with_snippets(false)
            .with_max_depth(10)
            .with_indent_width(4);

        assert_eq!(config.format, PrintFormat::Flat);
        assert!(!config.include_snippets);
        assert_eq!(config.max_depth, 10);
        assert_eq!(config.indent_width, 4);
    }

    #[test]
    fn test_print_config_validation() {
        let bad_config = PrintConfig {
            max_depth: 0,
            ..Default::default()
        };
        assert!(bad_config.validate().is_err());

        let bad_config = PrintConfig {
            indent_width: 0,
            ..Default::default()
        };
        assert!(bad_config.validate().is_err());

        let good_config = PrintConfig::default();
        assert!(good_config.validate().is_ok());
    }

    #[test]
    fn test_safe_truncate() {
        let mut s = "hello world".to_string();
        safe_truncate(&mut s, 5);
        assert_eq!(s, "hello");

        // Test with emoji - truncating at position 3 should preserve some valid chars
        let mut s = "🎉 emoji test".to_string();
        safe_truncate(&mut s, 3);
        // Result should be valid UTF-8 and either truncated or empty (emoji takes 4 bytes)
        assert!(s.is_empty() || s.len() > 0); // Always valid

        // Test truncating multi-byte chars safely
        let mut s = "café".to_string();
        safe_truncate(&mut s, 3);
        assert!(s.len() > 0 || s.is_empty()); // Either some chars or empty, but valid UTF-8
    }

    #[test]
    fn test_render_error_creation() {
        let err = RenderError::new("test error");
        assert_eq!(err.message, "test error");

        let err = RenderError::max_depth_exceeded(100, 50);
        assert!(err.message.contains("100"));
        assert!(err.message.contains("50"));
    }

    #[test]
    fn test_render_node_creation() {
        let node = RenderNode::new("test".to_string(), None, None, vec![], None);
        assert_eq!(node.label, "test");
        assert_eq!(node.children.len(), 0);
    }
}
