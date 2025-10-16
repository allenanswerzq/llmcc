use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirId, HirNode};
use tree_sitter::Node;

/// Visibility as written in source for a function item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FnVisibility {
    Private,
    Public,
    Crate,
    Restricted(String),
}

/// Captured argument information (pattern plus optional type).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionParameter {
    pub pattern: String,
    pub ty: Option<TypeExpr>,
}

/// Rich metadata extracted for each Rust function encountered in the HIR.
#[derive(Debug, Clone)]
pub struct FunctionDescriptor {
    /// HIR identifier for the function node.
    pub hir_id: HirId,
    /// Short name as written in source (no qualification).
    pub name: String,
    /// Module/impl/trait context owning the function.
    // pub owner: SymbolOwner,
    /// Visibility as declared (`pub`, `pub(crate)`, etc.).
    pub visibility: FnVisibility,
    /// Whether the function is marked `async`.
    pub is_async: bool,
    /// Whether the function is marked `const`.
    pub is_const: bool,
    /// Whether the function is marked `unsafe`.
    pub is_unsafe: bool,
    /// Generic parameter clause (`<T: Foo>`), if present.
    pub generics: Option<String>,
    /// Optional `where` clause.
    pub where_clause: Option<String>,
    /// Ordered function parameters.
    pub parameters: Vec<FunctionParameter>,
    /// Parsed return type, if any.
    pub return_type: Option<TypeExpr>,
    /// Textual signature up to (but not including) the body.
    pub signature: String,
    /// Fully-qualified name derived from the owner chain.
    pub fqn: String,
}

impl FunctionDescriptor {
    pub fn from_hir<'tcx>(
        unit: CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        fqn: String,
    ) -> Option<Self> {
        let ts_node = match node.inner_ts_node() {
            ts if ts.kind() == "function_item" => ts,
            _ => return None,
        };

        let name_node = ts_node.child_by_field_name("name")?;
        let name = node_text(unit, name_node);
        let header_text = unit
            .file()
            .get_text(ts_node.start_byte(), name_node.start_byte());
        let fn_index = header_text.rfind("fn").unwrap_or_else(|| header_text.len());
        let header_clean = clean(&header_text[..fn_index]);

        // let owner = SymbolOwner::from_ts_node(unit, ts_node);
        let visibility = FnVisibility::from_header(&header_clean);
        let is_async = header_clean
            .split_whitespace()
            .any(|token| token == "async");
        let is_const = header_clean
            .split_whitespace()
            .any(|token| token == "const");
        let is_unsafe = header_clean
            .split_whitespace()
            .any(|token| token == "unsafe");
        let body_start = ts_node
            .child_by_field_name("body")
            .map(|body| body.start_byte())
            .unwrap_or_else(|| ts_node.end_byte());
        let signature = clean(&unit.file().get_text(ts_node.start_byte(), body_start));

        let generics = ts_node
            .child_by_field_name("type_parameters")
            .map(|n| node_text(unit, n));
        let where_clause = ts_node
            .child_by_field_name("where_clause")
            .map(|n| node_text(unit, n))
            .or_else(|| extract_where_clause(&signature));
        let parameters = ts_node
            .child_by_field_name("parameters")
            .map(|n| parse_parameters(unit, n))
            .unwrap_or_default();
        let return_type = ts_node
            .child_by_field_name("return_type")
            .map(|n| parse_return_type_node(unit, n));

        Some(FunctionDescriptor {
            hir_id: node.hir_id(),
            name,
            // owner,
            visibility,
            is_async,
            is_const,
            is_unsafe,
            generics,
            where_clause,
            parameters,
            return_type,
            signature,
            fqn,
        })
    }
}

fn extract_where_clause(signature: &str) -> Option<String> {
    let signature = signature.trim();
    let idx = signature.find("where ")?;
    let clause = signature[idx..].trim();
    if clause.is_empty() {
        return None;
    }
    Some(clause.trim_end_matches(',').to_string())
}

impl FnVisibility {
    pub(crate) fn from_header(header: &str) -> Self {
        if let Some(index) = header.find("pub") {
            let rest = &header[index..];
            let compressed: String = rest.chars().filter(|c| !c.is_whitespace()).collect();
            if compressed.starts_with("pub(") && compressed.ends_with(')') {
                let inner = &compressed[4..compressed.len() - 1];
                if inner == "crate" {
                    FnVisibility::Crate
                } else {
                    FnVisibility::Restricted(inner.to_string())
                }
            } else {
                FnVisibility::Public
            }
        } else {
            FnVisibility::Private
        }
    }
}

/// Representation of Rust types encountered in signatures/fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpr {
    /// A bare path with optional generic arguments (e.g. `Result<T, E>`).
    Path {
        segments: Vec<String>,
        generics: Vec<TypeExpr>,
    },
    /// `&'a mut T` style references.
    Reference {
        is_mut: bool,
        lifetime: Option<String>,
        inner: Box<TypeExpr>,
    },
    /// Tuple types such as `(A, B)`.
    Tuple(Vec<TypeExpr>),
    /// `impl Trait` sugar captured verbatim.
    ImplTrait { bounds: String },
    /// Fallback for nodes we do not yet model.
    Unknown(String),
}

impl TypeExpr {
    pub fn path_segments(&self) -> Option<&[String]> {
        match self {
            TypeExpr::Path { segments, .. } => Some(segments),
            _ => None,
        }
    }

    pub fn generics(&self) -> Option<&[TypeExpr]> {
        match self {
            TypeExpr::Path { generics, .. } => Some(generics),
            _ => None,
        }
    }
}

fn parse_parameters<'tcx>(
    unit: CompileUnit<'tcx>,
    params_node: Node<'tcx>,
) -> Vec<FunctionParameter> {
    let mut params = Vec::new();
    let mut cursor = params_node.walk();
    for child in params_node.named_children(&mut cursor) {
        match child.kind() {
            "parameter" => {
                let pattern = child
                    .child_by_field_name("pattern")
                    .map(|n| node_text(unit, n))
                    .unwrap_or_else(|| node_text(unit, child));
                let ty = child
                    .child_by_field_name("type")
                    .map(|n| parse_type_expr(unit, n));
                params.push(FunctionParameter { pattern, ty });
            }
            "self_parameter" => {
                params.push(FunctionParameter {
                    pattern: node_text(unit, child),
                    ty: None,
                });
            }
            _ => {}
        }
    }
    params
}

pub(crate) fn parse_type_expr<'tcx>(unit: CompileUnit<'tcx>, node: Node<'tcx>) -> TypeExpr {
    match node.kind() {
        "type_identifier" | "primitive_type" => TypeExpr::Path {
            segments: node_text(unit, node)
                .split("::")
                .map(|s| s.to_string())
                .collect(),
            generics: Vec::new(),
        },
        "scoped_type_identifier" => TypeExpr::Path {
            segments: node_text(unit, node)
                .split("::")
                .map(|s| s.to_string())
                .collect(),
            generics: Vec::new(),
        },
        "generic_type" => parse_generic_type(unit, node),
        "reference_type" => parse_reference_type(unit, node),
        "tuple_type" => {
            let mut types = Vec::new();
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if is_type_node(child.kind()) {
                    types.push(parse_type_expr(unit, child));
                }
            }
            TypeExpr::Tuple(types)
        }
        "impl_trait_type" => TypeExpr::ImplTrait {
            bounds: node_text(unit, node),
        },
        _ => TypeExpr::Unknown(node_text(unit, node)),
    }
}

fn parse_generic_type<'tcx>(unit: CompileUnit<'tcx>, node: Node<'tcx>) -> TypeExpr {
    let mut base_segments: Vec<String> = Vec::new();
    let mut generics = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "type_identifier" | "scoped_type_identifier" => {
                base_segments = node_text(unit, child)
                    .split("::")
                    .map(|s| s.to_string())
                    .collect();
            }
            "type_arguments" => {
                generics = parse_type_arguments(unit, child);
            }
            _ => {}
        }
    }
    if base_segments.is_empty() {
        base_segments = node_text(unit, node)
            .split("::")
            .map(|s| s.to_string())
            .collect();
    }
    TypeExpr::Path {
        segments: base_segments,
        generics,
    }
}

fn parse_type_arguments<'tcx>(unit: CompileUnit<'tcx>, node: Node<'tcx>) -> Vec<TypeExpr> {
    let mut args = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "type_argument" => {
                if let Some(inner) = child.child_by_field_name("type") {
                    args.push(parse_type_expr(unit, inner));
                }
            }
            kind if is_type_node(kind) => {
                args.push(parse_type_expr(unit, child));
            }
            _ => {}
        }
    }
    args
}

fn parse_reference_type<'tcx>(unit: CompileUnit<'tcx>, node: Node<'tcx>) -> TypeExpr {
    let mut lifetime = None;
    let mut is_mut = false;
    let mut inner = None;
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "lifetime" => lifetime = Some(node_text(unit, child)),
            "mutable_specifier" => is_mut = true,
            kind if is_type_node(kind) => inner = Some(parse_type_expr(unit, child)),
            _ => {}
        }
    }
    let inner = inner.unwrap_or_else(|| TypeExpr::Unknown(node_text(unit, node)));
    TypeExpr::Reference {
        is_mut,
        lifetime,
        inner: Box::new(inner),
    }
}

fn is_type_node(kind: &str) -> bool {
    matches!(
        kind,
        "type_identifier"
            | "scoped_type_identifier"
            | "generic_type"
            | "reference_type"
            | "tuple_type"
            | "primitive_type"
            | "impl_trait_type"
    )
}

fn parse_return_type_node<'tcx>(unit: CompileUnit<'tcx>, node: Node<'tcx>) -> TypeExpr {
    let mut expr_opt = None;
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if expr_opt.is_none() && is_type_node(child.kind()) {
            expr_opt = Some(parse_type_expr(unit, child));
        }
    }

    let mut expr = if let Some(expr) = expr_opt {
        expr
    } else if let Some(inner) = node.child_by_field_name("type") {
        parse_type_expr(unit, inner)
    } else {
        parse_type_expr(unit, node)
    };

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "type_arguments" {
            let extra = parse_type_arguments(unit, child);
            if !extra.is_empty() {
                if let TypeExpr::Path { generics, .. } = &mut expr {
                    generics.extend(extra);
                }
            }
        }
    }

    if let TypeExpr::Unknown(text) = &expr {
        if text.starts_with("impl ") {
            expr = TypeExpr::ImplTrait {
                bounds: text.clone(),
            };
        }
    }

    expr
}

fn clean(text: &str) -> String {
    let mut out = String::new();
    let mut last_was_ws = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !last_was_ws && !out.is_empty() {
                out.push(' ');
            }
            last_was_ws = true;
        } else {
            out.push(ch);
            last_was_ws = false;
        }
    }
    out.trim().to_string()
}

fn node_text<'tcx>(unit: CompileUnit<'tcx>, node: Node<'tcx>) -> String {
    clean(&unit.file().get_text(node.start_byte(), node.end_byte()))
}
