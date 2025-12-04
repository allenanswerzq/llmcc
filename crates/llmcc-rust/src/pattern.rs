/// Pattern and type helpers for symbol binding.
///
/// This module provides functions to:
/// 1. Infer complete types from expressions (pattern type propagation)
/// 2. Assign inferred types to pattern variables recursively
/// 3. Collect identifiers from patterns
use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirId, HirKind, HirNode};
use llmcc_core::symbol::Symbol;
use llmcc_resolver::BinderScopes;

use crate::token::LangRust;
use crate::ty::TyCtxt;

/// Get inferred type of an expression/node by delegating to TyCtxt.
pub fn infer_type_from_node<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    let mut ty_ctxt = TyCtxt::new(unit, scopes);
    ty_ctxt.infer(node)
}

/// Assign inferred type to a pattern, recursively handling nested patterns.
///
/// This mirrors the C# `AssignTypeToPattern` logic, handling:
/// - Simple identifiers: direct type assignment
/// - Tuple patterns: distribute nested types by position
/// - Struct patterns: match field patterns to struct fields
/// - Ref/mut patterns: forward to inner pattern
/// - Or patterns: assign same type to all branches
pub fn assign_type_to_pattern<'tcx>(
    unit: &CompileUnit<'tcx>,
    _scopes: &BinderScopes<'tcx>,
    pattern_node: &HirNode<'tcx>,
    inferred_type: Option<&'tcx Symbol>,
) {
    if let Some(sym) = inferred_type {
        tracing::trace!(
            "assigning type '{}' to pattern {:?}",
            sym.format(Some(unit.interner())),
            pattern_node.id()
        );
    }

    // Check if this is an identifier - if so, bind it directly
    if pattern_node.kind_id() == LangRust::identifier {
        if let Some(ident) = pattern_node.as_ident()
            && let Some(sym) = inferred_type
        {
            if let Some(existing_sym) = ident.opt_symbol() {
                // Update type if not already set
                if existing_sym.type_of().is_none() {
                    existing_sym.set_type_of(sym.id());
                    tracing::trace!(
                        "bound identifier '{}' to type '{}'",
                        ident.name.as_str(),
                        sym.format(Some(unit.interner()))
                    );
                }
            }
        }
        return;
    }

    // For compound patterns, recursively process children
    // This handles: tuple patterns, struct patterns, ref/mut patterns, etc.
    let mut child_index = 0;
    for child in pattern_node.children(unit) {
        // Skip trivia nodes (Text, Comment)
        if matches!(child.kind(), HirKind::Text | HirKind::Comment) {
            continue;
        }

        // Determine what type to assign to this child
        let child_type = if let Some(tuple_sym) = inferred_type {
            // Try to get element type if this is a compound type
            if let Some(nested) = tuple_sym.nested_types() {
                nested
                    .get(child_index)
                    .and_then(|type_id| unit.opt_get_symbol(*type_id))
            } else {
                None
            }
        } else {
            None
        };

        assign_type_to_pattern(unit, _scopes, &child, child_type.or(inferred_type));
        child_index += 1;
    }
}

/// Collect all identifiers from a pattern node recursively.
/// Used for pattern analysis and type propagation.
#[allow(dead_code)]
pub fn collect_pattern_idents<'tcx>(
    unit: &CompileUnit<'tcx>,
    pattern: &HirNode<'tcx>,
) -> Vec<HirId> {
    let mut idents = Vec::new();
    collect_pattern_idents_recursive(unit, pattern, &mut idents);
    idents
}

fn collect_pattern_idents_recursive<'tcx>(
    unit: &CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    idents: &mut Vec<HirId>,
) {
    match node.kind_id() {
        LangRust::identifier | LangRust::shorthand_field_identifier => {
            idents.push(node.id());
        }
        _ => {
            for child in node.children(unit) {
                if !matches!(child.kind(), HirKind::Text | HirKind::Comment) {
                    collect_pattern_idents_recursive(unit, &child, idents);
                }
            }
        }
    }
}
