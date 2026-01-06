//! Pattern binding for TypeScript destructuring patterns.
//!
//! Handles:
//! - Array patterns: `let [a, b] = [1, 2]` or `[a, b] = arr`
//! - Object patterns: `let { x, y } = obj` or `({ x, y } = obj)`
//! - Rest patterns in arrays: `let [first, ...rest] = arr`
//! - Default values: `let { x = 0 } = obj`

#![allow(clippy::collapsible_if)]

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{SymKind, SymKindSet, Symbol};
use llmcc_resolver::BinderScopes;

use crate::token::LangTypeScript;

/// Bind types to identifiers within a destructuring pattern.
///
/// Given a pattern (array_pattern, object_pattern, or identifier) and a type,
/// recursively assigns types to bound variables.
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
        // AST: [a, b, c] or [a, ...rest]
        LangTypeScript::array_pattern => {
            assign_type_to_array_pattern(unit, scopes, pattern, pattern_type);
        }
        // AST: { x, y } or { x: a, y: b }
        LangTypeScript::object_pattern => {
            assign_type_to_object_pattern(unit, scopes, pattern, pattern_type);
        }
        // AST: ...rest
        LangTypeScript::rest_pattern => {
            // Rest pattern gets the same type (it will be an array of remaining elements)
            if let Some(inner) = pattern.find_ident(unit) {
                assign_type_to_ident(unit, scopes, inner, pattern_type);
            }
        }
        // AST: pattern = default_value
        LangTypeScript::assignment_pattern => {
            // The left side gets the type, ignore the default value
            if let Some(left) = pattern.child_by_field(unit, LangTypeScript::field_left) {
                bind_pattern_types(unit, scopes, &left, pattern_type);
            }
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
    unit: &CompileUnit<'tcx>,
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
                tracing::trace!(
                    "identifier '{}' missing symbol in pattern binding",
                    ident.name
                );
                return;
            }
        }
    };

    // Don't override existing type
    if symbol.type_of().is_some() {
        tracing::trace!(
            "identifier '{}' already has type, not overriding",
            ident.name
        );
        return;
    }

    symbol.set_type_of(ident_type.id());
    tracing::trace!(
        "assigned type to '{}': {}",
        ident.name,
        ident_type.format(Some(unit.interner()))
    );
}

/// AST: [a, b, c] or [first, ...rest]
/// Assign tuple/array element types to each pattern.
fn assign_type_to_array_pattern<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    pattern: &HirNode<'tcx>,
    pattern_type: &'tcx Symbol,
) {
    let nested_types = pattern_type.nested_types();

    let mut element_index = 0;
    for child in pattern.children(unit) {
        if child.is_trivia() {
            continue;
        }

        // Get the element type for this position
        let element_type = if let Some(ref types) = nested_types {
            // For tuple types [T1, T2, T3], get specific element type
            types
                .get(element_index)
                .and_then(|type_id| unit.opt_get_symbol(*type_id))
                .unwrap_or(pattern_type)
        } else {
            // For array types T[], all elements have the same type
            pattern_type
        };

        bind_pattern_types(unit, scopes, &child, element_type);
        element_index += 1;
    }

    tracing::trace!("assigned types to {} array pattern elements", element_index);
}

/// AST: { x, y } or { x: a, y: b } or { x: { nested } }
/// Bind each property pattern to the corresponding field type.
fn assign_type_to_object_pattern<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    pattern: &HirNode<'tcx>,
    pattern_type: &'tcx Symbol,
) {
    for child in pattern.children(unit) {
        let kind_id = child.kind_id();

        // Handle shorthand: { x, y } where property name is also the binding
        if kind_id == LangTypeScript::shorthand_property_identifier_pattern {
            if let Some(ident) = child.as_ident() {
                // Look up the field type from the pattern type
                if let Some(field_sym) = scopes.lookup_member_symbols(
                    pattern_type,
                    ident.name,
                    SymKindSet::from_kind(SymKind::Field),
                ) {
                    if let Some(field_type_id) = field_sym.type_of() {
                        if let Some(field_type) = unit.opt_get_symbol(field_type_id) {
                            assign_type_to_ident(unit, scopes, ident, field_type);
                        }
                    }
                }
            }
        }
        // Handle pair: { x: a } or { x: { nested } }
        else if kind_id == LangTypeScript::pair_pattern {
            // Get the key (property name) and value (binding pattern)
            if let Some(key_node) = child.child_by_field(unit, LangTypeScript::field_key)
                && let Some(value_node) = child.child_by_field(unit, LangTypeScript::field_value)
            {
                // Get the key name to look up field type
                if let Some(key_ident) = key_node.find_ident(unit) {
                    if let Some(field_sym) = scopes.lookup_member_symbols(
                        pattern_type,
                        key_ident.name,
                        SymKindSet::from_kind(SymKind::Field),
                    ) {
                        if let Some(field_type_id) = field_sym.type_of() {
                            if let Some(field_type) = unit.opt_get_symbol(field_type_id) {
                                // Recursively bind the value pattern
                                bind_pattern_types(unit, scopes, &value_node, field_type);
                            }
                        }
                    }
                }
            }
        }
        // Handle rest: { ...rest }
        else if kind_id == LangTypeScript::rest_pattern {
            // Rest gets the same object type (could refine to Omit<T, ...> but that's complex)
            if let Some(ident) = child.find_ident(unit) {
                assign_type_to_ident(unit, scopes, ident, pattern_type);
            }
        }
        // Handle assignment pattern with default: { x = 0 }
        else if kind_id == LangTypeScript::object_assignment_pattern {
            // Get the left side (the binding) and look up its field type
            if let Some(left) = child.child_by_field(unit, LangTypeScript::field_left) {
                if let Some(ident) = left.find_ident(unit) {
                    if let Some(field_sym) = scopes.lookup_member_symbols(
                        pattern_type,
                        ident.name,
                        SymKindSet::from_kind(SymKind::Field),
                    ) {
                        if let Some(field_type_id) = field_sym.type_of() {
                            if let Some(field_type) = unit.opt_get_symbol(field_type_id) {
                                assign_type_to_ident(unit, scopes, ident, field_type);
                            }
                        }
                    }
                }
            }
        }
    }

    tracing::trace!("assigned types to object pattern fields");
}
