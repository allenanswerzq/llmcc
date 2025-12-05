use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::Symbol;
use llmcc_resolver::BinderScopes;

use crate::token::LangRust;

#[tracing::instrument(skip_all)]
pub fn bind_pattern_types<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    pattern: &HirNode<'tcx>,
    pattern_type: Option<&'tcx Symbol>,
) {
    // Check if this is an identifier
    if let Some(ident) = pattern.as_ident() {
        assign_type_to_ident(unit, scopes, ident, pattern_type);
        return;
    }

    // Check the pattern kind
    match pattern.kind_id() {
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
    ident_type: Option<&'tcx Symbol>,
) {
    let default_type = ident_type.unwrap_or_else(|| {
        // Get auto/unknown type as fallback
        scopes.lookup_symbol(&"auto", vec![]).unwrap_or_else(|| {
            // If no auto type found, just return unknown
            scopes.lookup_symbol(&"_", vec![]).unwrap()
        })
    });

    if let Some(existing_sym) = ident.opt_symbol() {
        // Symbol already exists - check if it's a const (cannot redeclare)
        if existing_sym.kind().is_const() {
            tracing::trace!("const '{}' cannot be redeclared", ident.name);
            return;
        }

        // If already has a type, don't override (unless it's unresolved)
        if existing_sym.type_of().is_some() && !existing_sym.kind().is_unresolved() {
            tracing::trace!(
                "identifier '{}' already has type, not overriding",
                ident.name
            );
            return;
        }

        // Update type if not already set
        if existing_sym.type_of().is_none() {
            existing_sym.set_type_of(default_type.id());
            tracing::trace!(
                "assigned type to existing '{}': {}",
                ident.name,
                default_type.format(Some(unit.interner()))
            );
        }
    } else {
        // Create new symbol for this binding
        let new_sym = scopes.declare_symbol(&ident.name);
        new_sym.set_type_of(default_type.id());
        ident.set_symbol(new_sym);
        tracing::trace!(
            "created new binding '{}' with type {}",
            ident.name,
            default_type.format(Some(unit.interner()))
        );
    }
}

/// AST: (pattern1, pattern2, pattern3)
/// Assign tuple element types to each pattern
#[tracing::instrument(skip_all)]
fn assign_type_to_tuple_pattern<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    pattern: &HirNode<'tcx>,
    pattern_type: Option<&'tcx Symbol>,
) {
    if pattern_type.is_none() {
        tracing::trace!("tuple pattern has no type information");
        return;
    }

    let tuple_type = pattern_type.unwrap();
    let nested_types = tuple_type.nested_types();

    let mut element_index = 0;
    for child in pattern.children(unit) {
        // Skip text nodes (commas, parens)
        if child.kind_id() == LangRust::text_node {
            continue;
        }

        let element_type = nested_types
            .and_then(|types| types.get(element_index))
            .and_then(|type_id| unit.opt_get_symbol(*type_id));

        bind_pattern_types(unit, scopes, &child, element_type);
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
    _pattern_type: Option<&'tcx Symbol>,
) {
    // Find the struct type identifier
    let struct_type_node = match pattern.child_by_field(*unit, LangRust::field_type) {
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

    // Iterate through field patterns
    for child in pattern.children(unit) {
        if child.kind_id() == LangRust::field_pattern {
            // Get field name
            if let Some(field_name_node) = child.child_by_field(*unit, LangRust::field_name) {
                if let Some(field_name_ident) = field_name_node.find_ident(unit) {
                    // Try to find matching field in struct scope
                    if let Some(struct_scope_id) = struct_symbol.opt_scope() {
                        let field_scope = unit.get_scope(struct_scope_id);
                        if let Some(field_sym) = field_scope.lookup_symbol(&field_name_ident.name) {
                            let field_type = field_sym
                                .type_of()
                                .and_then(|type_id| unit.opt_get_symbol(type_id));

                            // Check for full pattern (field: pattern)
                            if let Some(inner_pattern) =
                                child.child_by_field(*unit, LangRust::field_pattern)
                            {
                                // Full pattern: { field: pattern }
                                bind_pattern_types(unit, scopes, &inner_pattern, field_type);
                            } else {
                                // Shorthand: { field } - bind the identifier directly
                                assign_type_to_ident(unit, scopes, field_name_ident, field_type);
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
}

/// AST: TupleVariant(a, b, c) or TupleStruct(x, y)
/// Assign nested types to each pattern element
#[tracing::instrument(skip_all)]
fn assign_type_to_tuple_struct_pattern<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    pattern: &HirNode<'tcx>,
    _pattern_type: Option<&'tcx Symbol>,
) {
    // Get the tuple struct type
    let type_node = match pattern.child_by_field(*unit, LangRust::field_type) {
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
        if child.field_id() == LangRust::field_type || child.kind_id() == LangRust::text_node {
            continue;
        }

        let element_type = nested_types
            .and_then(|types| types.get(element_index))
            .and_then(|type_id| unit.opt_get_symbol(*type_id));

        bind_pattern_types(unit, scopes, &child, element_type);
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
    pattern_type: Option<&'tcx Symbol>,
) {
    for child in pattern.children(unit) {
        // Skip text nodes (pipes)
        if child.kind_id() == LangRust::text_node {
            continue;
        }

        // Each alternative gets the same type
        bind_pattern_types(unit, scopes, &child, pattern_type);
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
    pattern_type: Option<&'tcx Symbol>,
) {
    let element_type = pattern_type
        .and_then(|slice_type| slice_type.nested_types())
        .and_then(|types| types.first())
        .and_then(|type_id| unit.opt_get_symbol(*type_id));

    for child in pattern.children(unit) {
        // Skip text nodes (brackets, commas)
        if child.kind_id() == LangRust::text_node {
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
    pattern_type: Option<&'tcx Symbol>,
) {
    // For reference patterns, try to get the referenced type
    let referenced_type = pattern_type.and_then(|ref_type| {
        // If it's a reference type, extract the inner type
        ref_type
            .nested_types()
            .and_then(|types| types.first())
            .and_then(|type_id| unit.opt_get_symbol(*type_id))
    });

    // Find the inner pattern (skip &, &mut keywords)
    if let Some(inner_pattern) = pattern.child_by_field(*unit, LangRust::field_pattern) {
        bind_pattern_types(
            unit,
            scopes,
            &inner_pattern,
            referenced_type.or(pattern_type),
        );
    } else {
        // Fallback: look for any identifier in children
        for child in pattern.children(unit) {
            if child.kind_id() != LangRust::text_node {
                bind_pattern_types(unit, scopes, &child, referenced_type.or(pattern_type));
            }
        }
    }

    tracing::trace!("assigned type to reference pattern");
}
