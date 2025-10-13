use llmcc_core::context::Context;
use llmcc_core::ir::{HirId, HirNode};
use tree_sitter::Node;

use super::function::parse_type_expr;

/// Classification for a variable-like binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableKind {
    Let,
    Const,
    Static,
}

/// Scope in which a variable lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableScope {
    Global,
    Local,
}

/// Metadata captured for `let` bindings, `const` items, and `static` items.
#[derive(Debug, Clone)]
pub struct VariableDescriptor {
    /// HIR identifier associated with the binding.
    pub hir_id: HirId,
    /// Name as written in source.
    pub name: String,
    /// Fully-qualified name derived from the current scope chain.
    pub fqn: String,
    /// Binding kind (`let`, `const`, or `static`).
    pub kind: VariableKind,
    /// Whether the binding is declared mutable.
    pub is_mut: bool,
    /// Whether the binding lives at module scope or inside a function.
    pub scope: VariableScope,
    /// Optional parsed type annotation.
    pub ty: Option<super::function::TypeExpr>,
}

impl VariableDescriptor {
    pub(crate) fn from_let<'tcx>(
        ctx: Context<'tcx>,
        node: &HirNode<'tcx>,
        name: String,
        fqn: String,
    ) -> Self {
        let ts_node = node.inner_ts_node();
        let ty = ts_node
            .child_by_field_name("type")
            .map(|n| parse_type_expr(ctx, n));
        let is_mut = has_mutable_specifier(ts_node);

        VariableDescriptor {
            hir_id: node.hir_id(),
            name,
            fqn,
            kind: VariableKind::Let,
            is_mut,
            scope: VariableScope::Local,
            ty,
        }
    }

    pub(crate) fn from_const_item<'tcx>(
        ctx: Context<'tcx>,
        node: &HirNode<'tcx>,
        name: String,
        fqn: String,
    ) -> Self {
        let ts_node = node.inner_ts_node();
        let ty = ts_node
            .child_by_field_name("type")
            .map(|n| parse_type_expr(ctx, n));

        VariableDescriptor {
            hir_id: node.hir_id(),
            name,
            fqn,
            kind: VariableKind::Const,
            is_mut: false,
            scope: VariableScope::Global,
            ty,
        }
    }

    pub(crate) fn from_static_item<'tcx>(
        ctx: Context<'tcx>,
        node: &HirNode<'tcx>,
        name: String,
        fqn: String,
    ) -> Self {
        let ts_node = node.inner_ts_node();
        let ty = ts_node
            .child_by_field_name("type")
            .map(|n| parse_type_expr(ctx, n));
        let is_mut = has_mutable_specifier(ts_node);

        VariableDescriptor {
            hir_id: node.hir_id(),
            name,
            fqn,
            kind: VariableKind::Static,
            is_mut,
            scope: VariableScope::Global,
            ty,
        }
    }
}

fn has_mutable_specifier(ts_node: Node<'_>) -> bool {
    if ts_node.child_by_field_name("mutable_specifier").is_some() {
        return true;
    }
    let mut cursor = ts_node.walk();
    if cursor.goto_first_child() {
        loop {
            if cursor.node().kind() == "mutable_specifier" {
                return true;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    false
}
