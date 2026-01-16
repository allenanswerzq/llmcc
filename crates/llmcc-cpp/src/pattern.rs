//! Pattern binding for C++ structured bindings (C++17).
//!
//! Handles:
//! - Structured bindings: `auto [a, b] = pair;` or `auto& [x, y, z] = tuple;`

#![allow(clippy::collapsible_if)]
#![allow(dead_code)]

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{SymKind, SymKindSet, Symbol};
use llmcc_resolver::BinderScopes;

use crate::token::LangCpp;

/// Bind types to identifiers within a structured binding pattern.
///
/// Given a pattern (structured_binding_declarator) and a type,
/// assigns types to bound variables.
#[tracing::instrument(skip_all)]
pub fn bind_pattern_types<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    pattern: &HirNode<'tcx>,
    pattern_type: &'tcx Symbol,
) {
    // Check if this is a simple identifier
    if let Some(ident) = pattern.as_ident() {
        assign_type_to_ident(unit, scopes, ident, pattern_type);
        return;
    }

    // Check the pattern kind
    match pattern.kind_id() {
        // AST: auto [a, b, c] = ...
        LangCpp::structured_binding_declarator => {
            assign_type_to_structured_binding(unit, scopes, pattern, pattern_type);
        }
        _ => {
            // For other patterns, try to find and bind any identifier
            if let Some(ident) = pattern.find_ident(unit) {
                assign_type_to_ident(unit, scopes, ident, pattern_type);
            }
        }
    }
}

/// Assign type to a single identifier binding.
fn assign_type_to_ident<'tcx>(
    _unit: &CompileUnit<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    ident: &'tcx llmcc_core::ir::HirIdent<'tcx>,
    ident_type: &'tcx Symbol,
) {
    let symbol = match ident.opt_symbol() {
        Some(sym) => sym,
        None => {
            // Try to look up the variable in scope
            let resolved =
                scopes.lookup_symbol(ident.name, SymKindSet::from_kind(SymKind::Variable));
            if let Some(sym) = resolved {
                ident.set_symbol(sym);
                sym
            } else {
                return;
            }
        }
    };

    // Don't override existing type
    if symbol.type_of().is_some() {
        return;
    }

    symbol.set_type_of(ident_type.id());
}

/// Assign types to structured binding: auto [a, b, c] = tuple;
fn assign_type_to_structured_binding<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    pattern: &HirNode<'tcx>,
    pattern_type: &'tcx Symbol,
) {
    // In C++, structured bindings decompose tuples, pairs, arrays, or structs
    // For now, we assign the container type to each element
    // A more sophisticated implementation would track tuple element types

    for child in pattern.children(unit) {
        if let Some(ident) = child.as_ident() {
            assign_type_to_ident(unit, scopes, ident, pattern_type);
        } else if let Some(ident) = child.find_ident(unit) {
            assign_type_to_ident(unit, scopes, ident, pattern_type);
        }
    }
}
