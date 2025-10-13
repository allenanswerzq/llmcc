use llmcc_core::context::Context;
use llmcc_core::ir::{HirId, HirNode};
use tree_sitter::Node;

use super::function::{parse_type_expr, TypeExpr};

/// Description of a function-style call expression discovered in the source.
#[derive(Debug, Clone)]
pub struct CallDescriptor {
    /// HIR identifier for the call expression.
    pub hir_id: HirId,
    /// Best-effort classification of the call target.
    pub target: CallTarget,
    /// Raw argument snippets in call order.
    pub arguments: Vec<CallArgument>,
    /// Fully-qualified name of the function/method that owns this call, if any.
    pub enclosing_function: Option<String>,
}

/// Information about the call target expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallTarget {
    /// A path-based call such as `foo::bar()`.
    Path {
        segments: Vec<String>,
        generics: Vec<TypeExpr>,
    },
    /// A method-style call `receiver.method()`.
    Method {
        receiver: String,
        method: String,
        generics: Vec<TypeExpr>,
    },
    /// Anything we could not recognise (stored verbatim).
    Unknown(String),
}

/// Lightweight view of each argument expression.
#[derive(Debug, Clone)]
pub struct CallArgument {
    pub text: String,
}

impl CallDescriptor {
    pub fn from_call<'tcx>(
        ctx: Context<'tcx>,
        node: &HirNode<'tcx>,
        enclosing_function: Option<String>,
    ) -> Self {
        let ts_node = node.inner_ts_node();
        let function_node = ts_node.child_by_field_name("function");
        let call_generics = ts_node
            .child_by_field_name("type_arguments")
            .map(|n| parse_type_arguments(ctx, n))
            .unwrap_or_default();
        let target = match function_node {
            Some(func) => parse_call_target(ctx, func, call_generics.clone()),
            None => CallTarget::Unknown(clean(&node_text(ctx, ts_node))),
        };

        let arguments = ts_node
            .child_by_field_name("arguments")
            .map(|args| parse_arguments(ctx, args))
            .unwrap_or_default();

        CallDescriptor {
            hir_id: node.hir_id(),
            target,
            arguments,
            enclosing_function,
        }
    }
}

fn parse_arguments<'tcx>(ctx: Context<'tcx>, args_node: Node<'tcx>) -> Vec<CallArgument> {
    let mut cursor = args_node.walk();
    args_node
        .named_children(&mut cursor)
        .map(|arg| CallArgument {
            text: clean(&node_text(ctx, arg)),
        })
        .collect()
}

fn parse_call_target<'tcx>(
    ctx: Context<'tcx>,
    node: Node<'tcx>,
    call_generics: Vec<TypeExpr>,
) -> CallTarget {
    match node.kind() {
        "identifier" | "scoped_identifier" | "type_identifier" => {
            let segments: Vec<String> = clean(&node_text(ctx, node))
                .split("::")
                .map(|s| s.to_string())
                .collect();
            CallTarget::Path {
                segments,
                generics: call_generics,
            }
        }
        "generic_type" => {
            let base = node.child_by_field_name("type").unwrap_or(node);
            let mut segments: Vec<String> = clean(&node_text(ctx, base))
                .split("::")
                .map(|s| s.to_string())
                .collect();
            if segments.is_empty() {
                segments.push(clean(&node_text(ctx, base)));
            }
            let generics = node
                .child_by_field_name("type_arguments")
                .map(|args| parse_type_arguments(ctx, args))
                .unwrap_or(call_generics);
            CallTarget::Path { segments, generics }
        }
        "field_expression" => {
            let receiver = node
                .child_by_field_name("argument")
                .map(|n| clean(&node_text(ctx, n)))
                .unwrap_or_else(|| clean(&node_text(ctx, node)));
            let method = node
                .child_by_field_name("field")
                .map(|n| clean(&node_text(ctx, n)))
                .unwrap_or_default();
            let generics = node
                .child_by_field_name("type_arguments")
                .map(|n| parse_type_arguments(ctx, n))
                .unwrap_or(call_generics);
            CallTarget::Method {
                receiver,
                method,
                generics,
            }
        }
        _ => {
            let text = clean(&node_text(ctx, node));
            if let Some((receiver, method)) = parse_method_from_text(&text) {
                CallTarget::Method {
                    receiver,
                    method,
                    generics: call_generics,
                }
            } else {
                CallTarget::Unknown(text)
            }
        }
    }
}

fn parse_type_arguments<'tcx>(ctx: Context<'tcx>, node: Node<'tcx>) -> Vec<TypeExpr> {
    let mut args = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "type_argument" => {
                if let Some(inner) = child.child_by_field_name("type") {
                    args.push(parse_type_expr(ctx, inner));
                }
            }
            kind if is_type_node(kind) => {
                args.push(parse_type_expr(ctx, child));
            }
            _ => {}
        }
    }
    args
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

fn node_text<'tcx>(ctx: Context<'tcx>, node: Node<'tcx>) -> String {
    ctx.file().get_text(node.start_byte(), node.end_byte())
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

fn parse_method_from_text(text: &str) -> Option<(String, String)> {
    let idx = text.rfind('.')?;
    let (receiver, method_part) = text.split_at(idx);
    Some((
        receiver.trim().to_string(),
        method_part.trim_start_matches('.').to_string(),
    ))
}
