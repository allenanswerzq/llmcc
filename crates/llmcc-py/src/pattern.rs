//! Pattern binding helpers for Python.
//!
//! Handles binding types to patterns for:
//! - Simple identifiers: x = value
//! - Tuple unpacking: a, b = value
//! - List unpacking: [a, b] = value
//! - Starred patterns: a, *rest = value
//! - match/case patterns (Python 3.10+)

#![allow(clippy::collapsible_if)]

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{SymKind, SymKindSet, Symbol};
use llmcc_resolver::BinderScopes;

use crate::token::LangPython;

/// Bind pattern types from a value type to a pattern.
/// This recursively handles nested patterns like tuple unpacking.
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
        // AST: (pattern1, pattern2, ...)
        LangPython::tuple_pattern | LangPython::pattern_list => {
            assign_type_to_tuple_pattern(unit, scopes, pattern, pattern_type);
        }
        // AST: [pattern1, pattern2, ...]
        LangPython::list_pattern => {
            assign_type_to_list_pattern(unit, scopes, pattern, pattern_type);
        }
        // AST: *rest (starred pattern in unpacking)
        LangPython::list_splat_pattern => {
            assign_type_to_starred_pattern(unit, scopes, pattern, pattern_type);
        }
        // AST: a, b, c (tuple without parens in assignment target)
        LangPython::tuple => {
            assign_type_to_tuple_pattern(unit, scopes, pattern, pattern_type);
        }
        // AST: [a, b, c] in assignment target
        LangPython::list => {
            assign_type_to_list_pattern(unit, scopes, pattern, pattern_type);
        }
        _ => {
            // Handle other patterns - find and assign to any identifiers
            if let Some(ident) = pattern.find_ident(unit) {
                assign_type_to_ident(unit, scopes, ident, pattern_type);
            } else {
                // Recurse into children
                for child in pattern.children(unit) {
                    bind_pattern_types(unit, scopes, &child, pattern_type);
                }
            }
        }
    }
}

/// Assign type to a single identifier binding
fn assign_type_to_ident<'tcx>(
    _unit: &CompileUnit<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    ident: &'tcx llmcc_core::ir::HirIdent<'tcx>,
    ident_type: &'tcx Symbol,
) {
    let default_type = ident_type;

    let symbol = match ident.opt_symbol() {
        Some(sym) => sym,
        None => {
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

    // Don't override const types
    if symbol.kind().is_const() {
        return;
    }

    // Don't override if already has a resolved type
    if symbol.type_of().is_some() && symbol.kind().is_resolved() {
        return;
    }

    // Set the type
    if symbol.type_of().is_none() {
        symbol.set_type_of(default_type.id());
    }
}

/// Assign types to tuple pattern elements
fn assign_type_to_tuple_pattern<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    pattern: &HirNode<'tcx>,
    tuple_type: &'tcx Symbol,
) {
    let children: Vec<_> = pattern
        .children(unit)
        .into_iter()
        .filter(|c| !c.is_trivia())
        .collect();

    // If the tuple type has nested types, try to match them up
    if let Some(nested_types) = tuple_type.nested_types() {
        for (i, child) in children.iter().enumerate() {
            // Handle starred pattern (*rest) - gets remaining elements
            if child.kind_id() == LangPython::list_splat_pattern {
                // For starred patterns, assign the original container type
                // In practice, this would need to create a list type of remaining elements
                bind_pattern_types(unit, scopes, child, tuple_type);
                continue;
            }

            if i < nested_types.len() {
                if let Some(elem_type) = unit.opt_get_symbol(nested_types[i]) {
                    bind_pattern_types(unit, scopes, child, elem_type);
                }
            }
        }
    } else {
        // No nested type info - assign the tuple type to all elements
        for child in children {
            bind_pattern_types(unit, scopes, &child, tuple_type);
        }
    }
}

/// Assign types to list pattern elements
fn assign_type_to_list_pattern<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    pattern: &HirNode<'tcx>,
    list_type: &'tcx Symbol,
) {
    let children: Vec<_> = pattern
        .children(unit)
        .into_iter()
        .filter(|c| !c.is_trivia())
        .collect();

    // For lists, all elements have the same type (the element type)
    let elem_type = if let Some(nested_types) = list_type.nested_types()
        && !nested_types.is_empty()
    {
        unit.opt_get_symbol(nested_types[0]).unwrap_or(list_type)
    } else {
        list_type
    };

    for child in children {
        // Handle starred pattern (*rest) - gets remaining elements as a list
        if child.kind_id() == LangPython::list_splat_pattern {
            bind_pattern_types(unit, scopes, &child, list_type);
            continue;
        }

        bind_pattern_types(unit, scopes, &child, elem_type);
    }
}

/// Assign type to starred pattern (*rest)
fn assign_type_to_starred_pattern<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    pattern: &HirNode<'tcx>,
    container_type: &'tcx Symbol,
) {
    // The starred pattern captures remaining elements into a list
    // For now, just assign the container type to the identifier inside
    if let Some(ident) = pattern.find_ident(unit) {
        assign_type_to_ident(unit, scopes, ident, container_type);
    }
}
