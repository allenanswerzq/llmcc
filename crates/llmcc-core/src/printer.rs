//! Debug rendering for AST, HIR, and block trees.
//!
//! This module turns compiler structures into a small render tree first, then
//! formats that tree. Keeping collection separate from formatting makes the
//! output modes consistent and keeps config behavior in one place.

use std::fmt::{self, Write as _};
use std::io::{self, Write as IoWrite};

use strum_macros::{Display, EnumString};

use crate::block::BasicBlock;
use crate::context::CompileUnit;
use crate::id::BlockId;
use crate::ir::{HirId, HirNode};
use crate::lang_def::ParseNode;
use crate::{Error, ErrorKind, Result};

const DEFAULT_SNIPPET_COLUMN: usize = 60;
const DEFAULT_SNIPPET_MAX_LEN: usize = 60;
const DEFAULT_MAX_DEPTH: usize = 1000;
const DEFAULT_INDENT_WIDTH: usize = 2;
const VERBOSE_SNIPPET_COLUMN: usize = 80;
const VERBOSE_SNIPPET_MAX_LEN: usize = 100;

/// Text format used when rendering debug trees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Display, EnumString)]
#[strum(serialize_all = "snake_case", ascii_case_insensitive)]
pub enum PrintFormat {
    /// Parenthesized tree with indentation.
    #[default]
    Tree,
    /// Parenthesized tree rendered onto one line.
    Compact,
    /// One node label per line in traversal order.
    Flat,
}

/// Rendered AST and HIR debug output for one compile unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrRender {
    /// Rendered parse tree, or a diagnostic placeholder when no parse tree exists.
    ast: String,
    /// Rendered HIR tree.
    hir: String,
}

impl IrRender {
    /// Create rendered IR output from AST and HIR strings.
    pub fn new(ast: impl Into<String>, hir: impl Into<String>) -> Self {
        Self {
            ast: ast.into(),
            hir: hir.into(),
        }
    }

    /// Return borrowed rendered sections.
    pub fn as_parts(&self) -> (&str, &str) {
        (&self.ast, &self.hir)
    }

    /// Return rendered parse tree output.
    pub fn ast(&self) -> &str {
        &self.ast
    }

    /// Return rendered HIR tree output.
    pub fn hir(&self) -> &str {
        &self.hir
    }

    /// Return owned rendered sections.
    pub fn into_parts(self) -> (String, String) {
        (self.ast, self.hir)
    }

    /// Write the rendered sections to `writer`.
    pub fn write_to(&self, mut writer: impl IoWrite) -> Result<()> {
        writeln!(writer, "{self}").map_err(write_failed)
    }
}

impl fmt::Display for IrRender {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "AST:\n{}\n\nHIR:\n{}", self.ast, self.hir)
    }
}

/// Options controlling debug rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintConfig {
    /// Output format.
    format: PrintFormat,
    /// Include compact source snippets next to rendered nodes.
    include_snippets: bool,
    /// Include one-indexed source line ranges.
    include_line_info: bool,
    /// Minimum column used to align snippets in tree format.
    snippet_col_width: usize,
    /// Maximum snippet length before truncation.
    snippet_max_length: usize,
    /// Maximum recursion depth from the root node. The root is depth 0.
    max_depth: usize,
    /// Spaces per indentation level in tree format.
    indent_width: usize,
    /// Include node ids when the source structure exposes them.
    include_node_ids: bool,
    /// Include parser field names in AST labels.
    include_field_names: bool,
    /// Maximum output line length; `0` means unlimited.
    line_width_limit: usize,
}

impl Default for PrintConfig {
    fn default() -> Self {
        Self {
            format: PrintFormat::Tree,
            include_snippets: true,
            include_line_info: true,
            snippet_col_width: DEFAULT_SNIPPET_COLUMN,
            snippet_max_length: DEFAULT_SNIPPET_MAX_LEN,
            max_depth: DEFAULT_MAX_DEPTH,
            indent_width: DEFAULT_INDENT_WIDTH,
            include_node_ids: false,
            include_field_names: false,
            line_width_limit: 0,
        }
    }
}

impl PrintConfig {
    /// Return the default render configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Use a specific output format.
    pub fn with_format(mut self, format: PrintFormat) -> Self {
        self.format = format;
        self
    }

    /// Enable or disable source snippets.
    pub fn with_snippets(mut self, enabled: bool) -> Self {
        self.include_snippets = enabled;
        self
    }

    /// Enable or disable source line ranges.
    pub fn with_line_info(mut self, enabled: bool) -> Self {
        self.include_line_info = enabled;
        self
    }

    /// Set the tree-format snippet alignment column.
    pub fn with_snippet_width(mut self, width: usize) -> Self {
        self.snippet_col_width = width;
        self
    }

    /// Set the maximum displayed snippet length.
    pub fn with_snippet_max_length(mut self, length: usize) -> Self {
        self.snippet_max_length = length;
        self
    }

    /// Set the maximum recursion depth from the root node. The root is depth 0.
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// Set the tree-format indentation width.
    pub fn with_indent_width(mut self, width: usize) -> Self {
        self.indent_width = width;
        self
    }

    /// Enable or disable node ids.
    pub fn with_node_ids(mut self, enabled: bool) -> Self {
        self.include_node_ids = enabled;
        self
    }

    /// Enable or disable parser field names in AST labels.
    pub fn with_field_names(mut self, enabled: bool) -> Self {
        self.include_field_names = enabled;
        self
    }

    /// Set the maximum rendered line length; `0` means unlimited.
    pub fn with_line_width_limit(mut self, width: usize) -> Self {
        self.line_width_limit = width;
        self
    }

    /// Small, fast output useful for smoke diagnostics.
    pub fn minimal() -> Self {
        Self {
            format: PrintFormat::Flat,
            include_snippets: false,
            include_line_info: false,
            include_node_ids: false,
            include_field_names: false,
            ..Self::default()
        }
    }

    /// Verbose output useful when inspecting parser or lowering issues.
    pub fn verbose() -> Self {
        Self {
            format: PrintFormat::Tree,
            include_snippets: true,
            include_line_info: true,
            include_node_ids: true,
            include_field_names: true,
            snippet_col_width: VERBOSE_SNIPPET_COLUMN,
            snippet_max_length: VERBOSE_SNIPPET_MAX_LEN,
            ..Self::default()
        }
    }

    /// Dense output that still keeps structural parentheses.
    pub fn compact() -> Self {
        Self {
            format: PrintFormat::Compact,
            include_snippets: true,
            include_line_info: true,
            include_node_ids: false,
            ..Self::default()
        }
    }

    /// Validate configuration invariants before rendering.
    ///
    /// Public rendering functions call this automatically; call it directly
    /// when checking a config before passing it elsewhere.
    pub fn validate(&self) -> Result<()> {
        if self.max_depth == 0 {
            return Err(invalid_config("max_depth must be greater than 0"));
        }
        if self.indent_width == 0 {
            return Err(invalid_config("indent_width must be greater than 0"));
        }
        if self.snippet_col_width == 0 {
            return Err(invalid_config("snippet_col_width must be greater than 0"));
        }
        if self.snippet_max_length == 0 {
            return Err(invalid_config("snippet_max_length must be greater than 0"));
        }
        Ok(())
    }
}

fn invalid_config(reason: impl Into<String>) -> Error {
    Error::new(ErrorKind::ConfigInvalid, reason)
        .with_operation("printer.validate")
        .with_context("component", "printer")
}

fn max_depth_exceeded(depth: usize, max_depth: usize) -> Error {
    Error::new(ErrorKind::InvalidArgument, "maximum render depth exceeded")
        .with_operation("printer.build_tree")
        .with_context("depth", depth.to_string())
        .with_context("max_depth", max_depth.to_string())
}

fn missing_hir_root() -> Error {
    Error::new(ErrorKind::InvariantViolation, "no HIR root node found")
        .with_operation("printer.print_ir")
}

fn write_failed(error: io::Error) -> Error {
    Error::new(ErrorKind::IoFailed, "failed to write printer output")
        .with_operation("printer.write")
        .with_context("component", "printer")
        .set_source(error)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SourceSpan {
    start_line: usize,
    end_line: usize,
}

impl SourceSpan {
    fn from_bytes(unit: CompileUnit<'_>, start_byte: usize, end_byte: usize) -> Self {
        let start_line = line_for_byte(unit, start_byte);
        let end_line = line_for_byte(unit, end_byte.saturating_sub(1));
        Self {
            start_line,
            end_line: end_line.max(start_line),
        }
    }
}

impl fmt::Display for SourceSpan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.start_line == self.end_line {
            write!(formatter, "[{}]", self.start_line)
        } else {
            write!(formatter, "[{}-{}]", self.start_line, self.end_line)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderNode {
    label: String,
    span: Option<SourceSpan>,
    snippet: Option<String>,
    children: Vec<RenderNode>,
    node_id: Option<String>,
}

impl RenderNode {
    fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            span: None,
            snippet: None,
            children: Vec::new(),
            node_id: None,
        }
    }

    fn with_span(mut self, span: SourceSpan) -> Self {
        self.span = Some(span);
        self
    }

    fn with_snippet(mut self, snippet: Option<String>) -> Self {
        self.snippet = snippet;
        self
    }

    fn with_children(mut self, children: Vec<RenderNode>) -> Self {
        self.children = children;
        self
    }

    fn with_id(mut self, node_id: impl Into<String>) -> Self {
        self.node_id = Some(node_id.into());
        self
    }
}

struct TreeBuilder<'cfg> {
    config: &'cfg PrintConfig,
}

impl<'cfg> TreeBuilder<'cfg> {
    fn new(config: &'cfg PrintConfig) -> Self {
        Self { config }
    }

    fn ast_tree(&self, unit: CompileUnit<'_>) -> Result<RenderNode> {
        let Some(parse_tree) = unit.try_parse_tree() else {
            return Ok(RenderNode::new(
                "parse tree not available for this compilation unit",
            ));
        };

        let root = parse_tree.root();
        self.ast_node(&*root, None, 0, unit, 0)
    }

    fn ast_node(
        &self,
        node: &dyn ParseNode,
        parent: Option<&dyn ParseNode>,
        child_index: usize,
        unit: CompileUnit<'_>,
        depth: usize,
    ) -> Result<RenderNode> {
        self.check_depth(depth)?;

        let field_name = self
            .config
            .include_field_names
            .then(|| parent.and_then(|parent| parent.child_field_name(child_index)))
            .flatten();

        let mut children = Vec::with_capacity(node.child_count());
        for index in 0..node.child_count() {
            let Some(child) = node.child(index) else {
                continue;
            };
            children.push(self.ast_node(&*child, Some(node), index, unit, depth + 1)?);
        }

        Ok(RenderNode::new(node.label(field_name))
            .with_span(SourceSpan::from_bytes(
                unit,
                node.start_byte(),
                node.end_byte(),
            ))
            .with_snippet(snippet_from_unit(
                unit,
                node.start_byte(),
                node.end_byte(),
                self.config,
            ))
            .with_children(children))
    }

    fn hir_tree(&self, root: HirId, unit: CompileUnit<'_>) -> Result<RenderNode> {
        let root = unit.hir_node(root);
        self.hir_node(&root, unit, 0)
    }

    fn hir_node(
        &self,
        node: &HirNode<'_>,
        unit: CompileUnit<'_>,
        depth: usize,
    ) -> Result<RenderNode> {
        self.check_depth(depth)?;

        let mut children = Vec::with_capacity(node.child_count());
        for child_id in node.child_ids() {
            let child = unit.hir_node(*child_id);
            children.push(self.hir_node(&child, unit, depth + 1)?);
        }

        let mut render = RenderNode::new(hir_label(node)).with_children(children);

        if let Some(base) = node.try_base() {
            render = render
                .with_id(format!("hir:{}", base.id))
                .with_span(SourceSpan::from_bytes(unit, base.start_byte, base.end_byte))
                .with_snippet(snippet_from_unit(
                    unit,
                    base.start_byte,
                    base.end_byte,
                    self.config,
                ));
        }

        Ok(render)
    }

    fn block_tree(&self, root: BlockId, unit: CompileUnit<'_>) -> Result<RenderNode> {
        let block = unit.block(root);
        self.block_node(&block, unit, 0)
    }

    fn block_node(
        &self,
        block: &BasicBlock<'_>,
        unit: CompileUnit<'_>,
        depth: usize,
    ) -> Result<RenderNode> {
        self.check_depth(depth)?;

        let mut children = Vec::with_capacity(block.children().len());
        for child_id in block.children() {
            let child = unit.block(child_id);
            children.push(self.block_node(&child, unit, depth + 1)?);
        }

        let node = block.node();
        Ok(RenderNode::new(block.to_string())
            .with_id(format!("block:{}", block.id()))
            .with_span(SourceSpan::from_bytes(
                unit,
                node.start_byte(),
                node.end_byte(),
            ))
            .with_snippet(snippet_from_unit(
                unit,
                node.start_byte(),
                node.end_byte(),
                self.config,
            ))
            .with_children(children))
    }

    fn check_depth(&self, depth: usize) -> Result<()> {
        if depth > self.config.max_depth {
            return Err(max_depth_exceeded(depth, self.config.max_depth));
        }
        Ok(())
    }
}

struct TreeWriter<'cfg> {
    config: &'cfg PrintConfig,
}

impl<'cfg> TreeWriter<'cfg> {
    fn new(config: &'cfg PrintConfig) -> Self {
        Self { config }
    }

    fn render(&self, root: &RenderNode) -> String {
        match self.config.format {
            PrintFormat::Tree => self.render_tree(root),
            PrintFormat::Compact => self.render_compact(root),
            PrintFormat::Flat => self.render_flat(root),
        }
    }

    fn render_tree(&self, root: &RenderNode) -> String {
        let mut lines = Vec::new();
        self.push_tree_node(root, 0, &mut lines);
        lines.join("\n")
    }

    fn push_tree_node(&self, node: &RenderNode, depth: usize, lines: &mut Vec<String>) {
        let indent = " ".repeat(depth * self.config.indent_width);
        let mut line = format!("{indent}({}", self.node_header(node));
        self.push_snippet(&mut line, node, true);

        if node.children.is_empty() {
            line.push(')');
            self.push_line(lines, line);
            return;
        }

        self.push_line(lines, line);
        for child in &node.children {
            self.push_tree_node(child, depth + 1, lines);
        }
        self.push_line(lines, format!("{indent})"));
    }

    fn render_compact(&self, root: &RenderNode) -> String {
        let mut line = String::new();
        self.write_compact_node(root, &mut line);
        limit_line(line, self.config.line_width_limit)
    }

    fn write_compact_node(&self, node: &RenderNode, output: &mut String) {
        output.push('(');
        output.push_str(&self.node_header(node));
        self.push_snippet(output, node, false);
        for child in &node.children {
            output.push(' ');
            self.write_compact_node(child, output);
        }
        output.push(')');
    }

    fn render_flat(&self, root: &RenderNode) -> String {
        let mut lines = Vec::new();
        self.push_flat_node(root, &mut lines);
        lines.join("\n")
    }

    fn push_flat_node(&self, node: &RenderNode, lines: &mut Vec<String>) {
        let mut line = self.node_header(node);
        self.push_snippet(&mut line, node, false);
        self.push_line(lines, line);

        for child in &node.children {
            self.push_flat_node(child, lines);
        }
    }

    fn node_header(&self, node: &RenderNode) -> String {
        let mut label = node.label.clone();

        if self.config.include_node_ids
            && let Some(node_id) = &node.node_id
        {
            let _ = write!(label, " #{node_id}");
        }

        if self.config.include_line_info
            && let Some(span) = node.span
        {
            let _ = write!(label, " {span}");
        }

        label
    }

    fn push_snippet(&self, line: &mut String, node: &RenderNode, align: bool) {
        let Some(snippet) = &node.snippet else {
            return;
        };

        if align && self.config.line_width_limit == 0 {
            let padding = self.config.snippet_col_width.saturating_sub(line.len());
            line.push_str(&" ".repeat(padding.max(1)));
        } else {
            line.push(' ');
        }

        let _ = write!(line, "|{snippet}|");
    }

    fn push_line(&self, lines: &mut Vec<String>, line: String) {
        lines.push(limit_line(line, self.config.line_width_limit));
    }
}

/// Render AST and HIR debug trees with default configuration.
pub fn render_ir(root: HirId, unit: CompileUnit<'_>) -> Result<IrRender> {
    render_ir_with(root, unit, &PrintConfig::default())
}

/// Render AST and HIR debug trees with custom configuration.
pub fn render_ir_with(
    root: HirId,
    unit: CompileUnit<'_>,
    config: &PrintConfig,
) -> Result<IrRender> {
    config.validate()?;

    let builder = TreeBuilder::new(config);
    let writer = TreeWriter::new(config);
    let ast = writer.render(&builder.ast_tree(unit)?);
    let hir = writer.render(&builder.hir_tree(root, unit)?);

    Ok(IrRender::new(ast, hir))
}

/// Write AST and HIR debug trees to `writer`.
pub fn write_ir(unit: CompileUnit<'_>, writer: impl IoWrite) -> Result<()> {
    write_ir_with(unit, &PrintConfig::default(), writer)
}

/// Write AST and HIR debug trees to `writer` with custom configuration.
pub fn write_ir_with(
    unit: CompileUnit<'_>,
    config: &PrintConfig,
    writer: impl IoWrite,
) -> Result<()> {
    let root = unit.try_file_root_id().ok_or_else(missing_hir_root)?;
    render_ir_with(root, unit, config)?.write_to(writer)
}

/// Print AST and HIR debug trees to stdout.
pub fn print_ir(unit: CompileUnit<'_>) -> Result<()> {
    print_ir_with(unit, &PrintConfig::default())
}

/// Print AST and HIR debug trees to stdout with custom configuration.
pub fn print_ir_with(unit: CompileUnit<'_>, config: &PrintConfig) -> Result<()> {
    let stdout = io::stdout();
    write_ir_with(unit, config, stdout.lock())
}

/// Render a block tree with default configuration.
pub fn render_block_tree(root: BlockId, unit: CompileUnit<'_>) -> Result<String> {
    render_block_tree_with(root, unit, &PrintConfig::default())
}

/// Render a block tree with custom configuration.
pub fn render_block_tree_with(
    root: BlockId,
    unit: CompileUnit<'_>,
    config: &PrintConfig,
) -> Result<String> {
    config.validate()?;

    let tree = TreeBuilder::new(config).block_tree(root, unit)?;
    Ok(TreeWriter::new(config).render(&tree))
}

/// Write a block tree to `writer`.
pub fn write_block_tree(root: BlockId, unit: CompileUnit<'_>, writer: impl IoWrite) -> Result<()> {
    write_block_tree_with(root, unit, &PrintConfig::default(), writer)
}

/// Write a block tree to `writer` with custom configuration.
pub fn write_block_tree_with(
    root: BlockId,
    unit: CompileUnit<'_>,
    config: &PrintConfig,
    mut writer: impl IoWrite,
) -> Result<()> {
    let graph = render_block_tree_with(root, unit, config)?;
    writeln!(writer, "{graph}").map_err(write_failed)
}

/// Print a block tree to stdout.
pub fn print_block_tree(root: BlockId, unit: CompileUnit<'_>) -> Result<()> {
    print_block_tree_with(root, unit, &PrintConfig::default())
}

/// Print a block tree to stdout with custom configuration.
pub fn print_block_tree_with(
    root: BlockId,
    unit: CompileUnit<'_>,
    config: &PrintConfig,
) -> Result<()> {
    let stdout = io::stdout();
    write_block_tree_with(root, unit, config, stdout.lock())
}

fn hir_label(node: &HirNode<'_>) -> String {
    let mut label = node.kind().to_string();
    if let HirNode::Ident(ident) = node {
        let _ = write!(label, " = \"{}\"", ident.name);
    }
    label
}

fn snippet_from_unit(
    unit: CompileUnit<'_>,
    start_byte: usize,
    end_byte: usize,
    config: &PrintConfig,
) -> Option<String> {
    if !config.include_snippets {
        return None;
    }

    unit.file()
        .try_get_text(start_byte, end_byte)
        .map(collapse_whitespace)
        .filter(|snippet| !snippet.is_empty())
        .map(|snippet| truncate_text(&snippet, config.snippet_max_length))
}

fn collapse_whitespace(text: impl AsRef<str>) -> String {
    text.as_ref()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn line_for_byte(unit: CompileUnit<'_>, byte_pos: usize) -> usize {
    let content = unit.file().content();
    let end = byte_pos.min(content.len());
    content[..end].iter().filter(|byte| **byte == b'\n').count() + 1
}

fn limit_line(line: String, limit: usize) -> String {
    if limit == 0 {
        return line;
    }

    truncate_text(&line, limit)
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }

    if max_chars <= 3 {
        return text.chars().take(max_chars).collect();
    }

    let mut truncated: String = text.chars().take(max_chars - 3).collect();
    truncated.push_str("...");
    truncated
}
