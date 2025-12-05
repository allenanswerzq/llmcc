#![allow(clippy::collapsible_if)]

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_resolver::BinderScopes;

use crate::infer::infer_type;
use crate::token::LangRust;

#[tracing::instrument(skip_all)]
pub fn bind_pattern_types<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    pattern: &HirNode<'tcx>,
    pattern_type: &'tcx Symbol,
) {
    // Check if this is an identifier
    if let Some(ident) = pattern.as_ident() {
        assign_type_to_ident(unit, scopes, ident, pattern_type);
        return;
    }

    // Check the pattern kind
    match pattern.kind_id() {
        // AST: (Type1, Type2, ...)
        LangRust::tuple_type => {
            if bind_tuple_type_to_pattern(unit, scopes, pattern) {
                return;
            }
        }
        // AST: (pattern1, pattern2, ...)
        LangRust::tuple_pattern => {
            assign_type_to_tuple_pattern(unit, scopes, pattern, pattern_type);
        }
        // AST: Struct { field1, field2, ... }
        LangRust::struct_pattern => {
            assign_type_to_struct_pattern(unit, scopes, pattern, pattern_type);
        }
        // AST: TupleStruct(a, b, c)
        LangRust::tuple_struct_pattern => {
            assign_type_to_tuple_struct_pattern(unit, scopes, pattern, pattern_type);
        }
        // AST: pattern1 | pattern2
        LangRust::or_pattern => {
            assign_type_to_or_pattern(unit, scopes, pattern, pattern_type);
        }
        // AST: [element1, element2, ...]
        LangRust::slice_pattern => {
            assign_type_to_slice_pattern(unit, scopes, pattern, pattern_type);
        }
        // AST: &pattern or &mut pattern
        LangRust::reference_pattern => {
            assign_type_to_reference_pattern(unit, scopes, pattern, pattern_type);
        }
        // AST: mut pattern or ref pattern
        LangRust::mut_pattern | LangRust::ref_pattern => {
            // Unwrap the mutable/ref modifier and process inner pattern
            if let Some(inner) = pattern.children(unit).first() {
                bind_pattern_types(unit, scopes, inner, pattern_type);
            }
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
#[tracing::instrument(skip_all)]
fn assign_type_to_ident<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    ident: &'tcx llmcc_core::ir::HirIdent<'tcx>,
    ident_type: &'tcx Symbol,
) {
    let default_type = ident_type;

    let symbol = match ident.opt_symbol() {
        Some(sym) => sym,
        None => {
            let resolved = scopes.lookup_symbol(&ident.name, vec![SymKind::Variable]);
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

    if symbol.kind().is_const() {
        tracing::trace!("const '{}' cannot be redeclared", ident.name);
        return;
    }

    if symbol.type_of().is_some() && symbol.kind().is_resolved() {
        tracing::trace!(
            "identifier '{}' already has type, not overriding",
            ident.name
        );
        return;
    }

    if symbol.type_of().is_none() {
        symbol.set_type_of(default_type.id());
        tracing::trace!(
            "assigned type to existing '{}': {}",
            ident.name,
            default_type.format(Some(unit.interner()))
        );
    }
}

#[tracing::instrument(skip_all)]
fn bind_tuple_type_to_pattern<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    type_node: &HirNode<'tcx>,
) -> bool {
    let Some(pattern) = type_node.child_by_field_recursive(unit, LangRust::field_pattern) else {
        return false;
    };

    let mut type_elems = type_node
        .children(unit)
        .into_iter()
        .filter(|child| !child.is_trivia());

    for child_pattern in pattern.children(unit) {
        if child_pattern.is_trivia() {
            continue;
        }

        if let Some(type_elem_node) = type_elems.next()
            && let Some(elem_sym) = infer_type(unit, scopes, &type_elem_node)
        {
            bind_pattern_types(unit, scopes, &child_pattern, elem_sym);
        }
    }

    true
}

/// AST: (pattern1, pattern2, pattern3)
/// Assign tuple element types to each pattern
#[tracing::instrument(skip_all)]
fn assign_type_to_tuple_pattern<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    pattern: &HirNode<'tcx>,
    pattern_type: &'tcx Symbol,
) {
    let tuple_type = pattern_type;
    let nested_types = tuple_type.nested_types();

    let mut element_index = 0;
    for child in pattern.children(unit) {
        if child.is_trivia() {
            continue;
        }

        if let Some(ref types) = nested_types {
            if let Some(type_id) = types.get(element_index) {
                if let Some(element_type) = unit.opt_get_symbol(*type_id) {
                    bind_pattern_types(unit, scopes, &child, element_type);
                }
            }
        }
        element_index += 1;
    }

    tracing::trace!("assigned types to {} tuple elements", element_index);
}

/// AST: Struct { field1, field2, ... }
/// Bind each field pattern to the struct field's type
#[tracing::instrument(skip_all)]
fn assign_type_to_struct_pattern<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    pattern: &HirNode<'tcx>,
    _pattern_type: &'tcx Symbol,
) {
    // Find the struct type identifier
    let struct_type_node = match pattern.child_by_field(unit, LangRust::field_type) {
        Some(node) => node,
        None => {
            tracing::trace!("struct pattern missing type field");
            return;
        }
    };

    let struct_type_ident = match struct_type_node.find_ident(unit) {
        Some(ident) => ident,
        None => {
            tracing::trace!("struct type node missing identifier");
            return;
        }
    };

    let struct_symbol = match struct_type_ident.opt_symbol() {
        Some(sym) => sym,
        None => {
            tracing::trace!("struct type identifier has no symbol");
            return;
        }
    };

    tracing::trace!(
        "struct pattern for type '{}'",
        struct_symbol.format(Some(unit.interner()))
    );

    for child in pattern.children(unit) {
        if child.kind_id() == LangRust::field_pattern {
            if let Some(field_name_node) = child.child_by_field(unit, LangRust::field_name) {
                if let Some(field_name_ident) = field_name_node.find_ident(unit) {
                    // Try to find matching field in struct scope
                    if let Some(field_sym) = scopes.lookup_member_symbols(
                        struct_symbol,
                        &field_name_ident.name,
                        Some(vec![SymKind::Field]),
                    ) {
                        let field_type = field_sym
                            .type_of()
                            .and_then(|type_id| unit.opt_get_symbol(type_id));

                        if let Some(inner_pattern) =
                            child.child_by_field(unit, LangRust::field_pattern)
                            && let Some(field_type) = field_type
                        // Check for full pattern (field: pattern)
                        {
                            bind_pattern_types(unit, scopes, &inner_pattern, field_type);
                        }
                        // Shorthand: { field } - bind the identifier directly
                        else if let Some(field_type) = field_type {
                            if let Some(binding_sym) = field_name_ident.opt_symbol() {
                                if binding_sym.kind() == SymKind::Variable {
                                    binding_sym.set_type_of(field_type.id());
                                } else if let Some(var_sym) = scopes
                                    .lookup_symbol(&field_name_ident.name, vec![SymKind::Variable])
                                {
                                    field_name_ident.set_symbol(var_sym);
                                    assign_type_to_ident(
                                        unit,
                                        scopes,
                                        field_name_ident,
                                        field_type,
                                    );
                                }
                            } else if let Some(var_sym) = scopes
                                .lookup_symbol(&field_name_ident.name, vec![SymKind::Variable])
                            {
                                field_name_ident.set_symbol(var_sym);
                                assign_type_to_ident(unit, scopes, field_name_ident, field_type);
                            }
                        }

                        tracing::trace!(
                            "bound struct field '{}' to pattern",
                            field_name_ident.name
                        );
                    }
                }
            }
        }
    }
}

/// AST: TupleVariant(a, b, c) or TupleStruct(x, y)
/// Assign nested types to each pattern element
#[tracing::instrument(skip_all)]
fn assign_type_to_tuple_struct_pattern<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    pattern: &HirNode<'tcx>,
    _pattern_type: &'tcx Symbol,
) {
    // Get the tuple struct type
    let type_node = match pattern.child_by_field(unit, LangRust::field_type) {
        Some(node) => node,
        None => return,
    };

    let type_ident = match type_node.find_ident(unit) {
        Some(ident) => ident,
        None => return,
    };

    let type_symbol = match type_ident.opt_symbol() {
        Some(sym) => sym,
        None => return,
    };

    let nested_types = type_symbol.nested_types();

    let mut element_index = 0;
    for child in pattern.children(unit) {
        // Skip the type field and text nodes
        if child.field_id() == LangRust::field_type || child.is_trivia() {
            continue;
        }

        if let Some(ref types) = nested_types {
            if let Some(type_id) = types.get(element_index) {
                if let Some(element_type) = unit.opt_get_symbol(*type_id) {
                    bind_pattern_types(unit, scopes, &child, element_type);
                }
            }
        }

        element_index += 1;
    }

    tracing::trace!("assigned types to {} tuple struct elements", element_index);
}

/// AST: pattern1 | pattern2 | pattern3
/// Each alternative gets the same type
#[tracing::instrument(skip_all)]
fn assign_type_to_or_pattern<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    pattern: &HirNode<'tcx>,
    pattern_type: &'tcx Symbol,
) {
    for child in pattern.children(unit) {
        if !child.is_trivia() {
            bind_pattern_types(unit, scopes, &child, pattern_type);
        }
    }

    tracing::trace!("assigned type to or-pattern alternatives");
}

/// AST: [elem1, elem2, ...] or [elem; size]
/// All elements get the same element type from the array/slice
#[tracing::instrument(skip_all)]
fn assign_type_to_slice_pattern<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    pattern: &HirNode<'tcx>,
    pattern_type: &'tcx Symbol,
) {
    // Extract the element type from the slice/array type
    let element_type = match pattern_type.nested_types() {
        Some(types) => {
            match types
                .first()
                .and_then(|type_id| unit.opt_get_symbol(*type_id))
            {
                Some(ty) => ty,
                None => {
                    tracing::trace!("slice pattern has no element type");
                    return;
                }
            }
        }
        None => {
            tracing::trace!("slice pattern has no nested types");
            return;
        }
    };

    for child in pattern.children(unit) {
        // Skip text nodes (brackets, commas, semicolons)
        if child.is_trivia() {
            continue;
        }

        bind_pattern_types(unit, scopes, &child, element_type);
    }

    tracing::trace!("assigned element type to slice pattern elements");
}

/// AST: &pattern or &mut pattern
/// Get the dereferenced type
#[tracing::instrument(skip_all)]
fn assign_type_to_reference_pattern<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    pattern: &HirNode<'tcx>,
    pattern_type: &'tcx Symbol,
) {
    // For reference patterns, try to get the referenced type
    let inner_type = match pattern_type.nested_types() {
        Some(types) => {
            match types
                .first()
                .and_then(|type_id| unit.opt_get_symbol(*type_id))
            {
                Some(ty) => ty,
                None => pattern_type,
            }
        }
        None => pattern_type,
    };

    // Find the inner pattern (skip &, &mut keywords)
    if let Some(inner_pattern) = pattern.child_by_field(unit, LangRust::field_pattern) {
        bind_pattern_types(unit, scopes, &inner_pattern, inner_type);
    } else {
        // Fallback: look for any identifier in children
        for child in pattern.children(unit) {
            if !child.is_trivia() {
                bind_pattern_types(unit, scopes, &child, inner_type);
            }
        }
    }

    tracing::trace!("assigned type to reference pattern");
}
