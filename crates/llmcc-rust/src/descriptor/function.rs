use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use tree_sitter::Node;

use llmcc_descriptor::{
    DescriptorOrigin, FunctionDescriptor, FunctionParameter, FunctionQualifiers, ParameterKind,
    SourceLocation, SourceSpan, TypeExpr, Visibility, LANGUAGE_RUST,
};

/// Build a language-agnostic function descriptor from a Rust function item.
pub fn from_hir<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    fqn: String,
) -> Option<FunctionDescriptor> {
    let ts_node = match node.inner_ts_node() {
        ts if ts.kind() == "function_item" => ts,
        _ => return None,
    };

    let name_node = ts_node.child_by_field_name("name")?;
    let name = clean(&node_text(unit, name_node));

    let header_text = unit
        .file()
        .get_text(ts_node.start_byte(), name_node.start_byte());
    let qualifiers = parse_qualifiers(&header_text);
    let visibility = parse_visibility(&header_text);

    let body_start = ts_node
        .child_by_field_name("body")
        .map(|body| body.start_byte())
        .unwrap_or_else(|| ts_node.end_byte());
    let signature = clean(&unit.file().get_text(ts_node.start_byte(), body_start));

    let generics = ts_node
        .child_by_field_name("type_parameters")
        .map(|n| clean(&node_text(unit, n)));
    let where_clause = ts_node
        .child_by_field_name("where_clause")
        .map(|n| clean(&node_text(unit, n)))
        .or_else(|| extract_where_clause(&signature));
    let parameters = find_parameters_node(ts_node)
        .map(|n| parse_parameters(unit, n))
        .unwrap_or_default();
    let return_type = ts_node
        .child_by_field_name("return_type")
        .and_then(|n| parse_return_type(unit, n));

    let origin = build_origin(unit, node, ts_node);

    let mut descriptor = FunctionDescriptor::new(origin, name);
    descriptor.fqn = Some(fqn);
    descriptor.visibility = visibility;
    descriptor.qualifiers = qualifiers;
    descriptor.generics = generics;
    descriptor.where_clause = where_clause;
    descriptor.parameters = parameters;
    descriptor.return_type = return_type;
    descriptor.signature = Some(signature);

    if std::env::var("LLMCC_DEBUG_PARAMS").is_ok() {
        eprintln!(
            "[llmcc_rust] built function descriptor name={} params={}",
            descriptor.name,
            descriptor.parameters.len()
        );
    }

    Some(descriptor)
}

pub(crate) fn build_origin<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    ts_node: Node<'tcx>,
) -> DescriptorOrigin {
    let span = SourceSpan::new(ts_node.start_byte() as u32, ts_node.end_byte() as u32);
    let file = unit
        .file_path()
        .or_else(|| unit.file().path())
        .map(|p| p.to_string());
    let location = SourceLocation::new(file, Some(span));

    DescriptorOrigin::new(LANGUAGE_RUST)
        .with_id(node.hir_id().0 as u64)
        .with_location(location)
}

pub(crate) fn parse_visibility(header: &str) -> Visibility {
    if let Some(index) = header.find("pub") {
        let rest = &header[index..];
        let compressed: String = rest.chars().filter(|c| !c.is_whitespace()).collect();

        if let Some(scope_start) = compressed.find("pub(") {
            let scope_expr = &compressed[scope_start + 4..];
            if let Some(scope_end) = scope_expr.find(')') {
                let scope = &scope_expr[..scope_end];
                if !scope.is_empty() {
                    return Visibility::restricted(scope);
                }
            }
        }

        Visibility::Public
    } else {
        Visibility::Private
    }
}

fn find_parameters_node<'tcx>(ts_node: Node<'tcx>) -> Option<Node<'tcx>> {
    if let Some(node) = ts_node.child_by_field_name("parameters") {
        return Some(node);
    }

    let mut cursor = ts_node.walk();
    for child in ts_node.named_children(&mut cursor) {
        if matches!(child.kind(), "parameters" | "parameter_list") {
            return Some(child);
        }
    }

    if std::env::var("LLMCC_DEBUG_PARAMS").is_ok() {
        eprintln!(
            "[llmcc_rust] missing parameters node: {}",
            ts_node.to_sexp()
        );
    }

    None
}

fn parse_qualifiers(header: &str) -> FunctionQualifiers {
    let mut qualifiers = FunctionQualifiers::default();
    for token in header.split_whitespace() {
        match token {
            "async" => qualifiers.is_async = true,
            "const" => qualifiers.is_const = true,
            "unsafe" => qualifiers.is_unsafe = true,
            _ => {}
        }
    }
    qualifiers
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

fn parse_parameters<'tcx>(
    unit: CompileUnit<'tcx>,
    params_node: Node<'tcx>,
) -> Vec<FunctionParameter> {
    if std::env::var("LLMCC_DEBUG_PARAMS").is_ok() {
        eprintln!(
            "[llmcc_rust] parsing parameters node kind={} sexp={}",
            params_node.kind(),
            params_node.to_sexp()
        );
    }
    let mut params = Vec::new();
    let mut cursor = params_node.walk();
    for child in params_node.named_children(&mut cursor) {
        match child.kind() {
            "parameter" => {
                let pattern = child
                    .child_by_field_name("pattern")
                    .map(|n| clean(&node_text(unit, n)))
                    .unwrap_or_else(|| clean(&node_text(unit, child)));
                let ty = child
                    .child_by_field_name("type")
                    .map(|n| parse_type_expr(unit, n));

                let mut param = FunctionParameter::new(identifier_from_pattern(&pattern));
                param.pattern = Some(pattern);
                param.type_hint = ty;
                params.push(param);
            }
            "self_parameter" => {
                let text = clean(&node_text(unit, child));
                let mut param = FunctionParameter::new(identifier_from_pattern(&text));
                param.pattern = Some(text);
                param.kind = ParameterKind::Receiver;
                params.push(param);
            }
            kind => {
                if std::env::var("LLMCC_DEBUG_PARAMS").is_ok() {
                    eprintln!("[llmcc_rust] unhandled parameter child kind: {}", kind);
                }
            }
        }
    }
    if params.is_empty() && std::env::var("LLMCC_DEBUG_PARAMS").is_ok() {
        eprintln!(
            "[llmcc_rust] empty parameters for node: {}",
            params_node.to_sexp()
        );
    }
    params
}

pub(crate) fn parse_type_expr<'tcx>(unit: CompileUnit<'tcx>, node: Node<'tcx>) -> TypeExpr {
    let expr = match node.kind() {
        "type_identifier" | "primitive_type" => TypeExpr::Path {
            segments: clean(&node_text(unit, node))
                .split("::")
                .map(|s| s.to_string())
                .collect(),
            generics: Vec::new(),
        },
        "scoped_type_identifier" => TypeExpr::Path {
            segments: clean(&node_text(unit, node))
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
            bounds: clean(&node_text(unit, node)),
        },
        _ => TypeExpr::Unknown(clean(&node_text(unit, node))),
    };

    if std::env::var("LLMCC_DEBUG_PARAMS").is_ok() {
        eprintln!(
            "[llmcc_rust] parse_type_expr kind={} expr={:?}",
            node.kind(),
            expr
        );
    }

    expr
}

fn parse_generic_type<'tcx>(unit: CompileUnit<'tcx>, node: Node<'tcx>) -> TypeExpr {
    let mut base_segments: Vec<String> = Vec::new();
    let mut generics = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "type_identifier" | "scoped_type_identifier" => {
                base_segments = clean(&node_text(unit, child))
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
        base_segments = clean(&node_text(unit, node))
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
            "lifetime" => lifetime = Some(clean(&node_text(unit, child))),
            "mutable_specifier" => is_mut = true,
            kind if is_type_node(kind) => inner = Some(parse_type_expr(unit, child)),
            _ => {}
        }
    }
    let inner = inner.unwrap_or_else(|| TypeExpr::Unknown(clean(&node_text(unit, node))));
    TypeExpr::Reference {
        is_mut,
        lifetime,
        inner: Box::new(inner),
    }
}

fn parse_return_type<'tcx>(unit: CompileUnit<'tcx>, node: Node<'tcx>) -> Option<TypeExpr> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if is_type_node(child.kind()) {
            return Some(parse_type_expr(unit, child));
        }
    }

    node.child_by_field_name("type")
        .map(|inner| parse_type_expr(unit, inner))
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

fn identifier_from_pattern(pattern: &str) -> Option<String> {
    let trimmed = pattern.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed
        .chars()
        .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '$')
        && !trimmed.chars().next().unwrap().is_numeric()
    {
        Some(trimmed.to_string())
    } else {
        None
    }
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
    unit.file().get_text(node.start_byte(), node.end_byte())
}
