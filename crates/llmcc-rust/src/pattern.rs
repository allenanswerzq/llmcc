use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{SymKind, SymKindSet, Symbol};
use llmcc_resolver::BindCtxt;

use crate::token::LangRust;

pub(crate) fn bind_pattern_types<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BindCtxt<'tcx>,
    pattern: &HirNode<'tcx>,
    pattern_type: &'tcx Symbol,
) {
    if let Some(ident) = pattern.as_ident() {
        assign_type_to_ident(unit, scopes, ident, pattern_type);
        return;
    }

    match pattern.kind_id() {
        LangRust::tuple_pattern => {
            assign_type_to_tuple_pattern(unit, scopes, pattern, pattern_type);
        }
        LangRust::struct_pattern => {
            assign_type_to_struct_pattern(unit, scopes, pattern, pattern_type);
        }
        LangRust::tuple_struct_pattern => {
            assign_type_to_tuple_struct_pattern(unit, scopes, pattern, pattern_type);
        }
        LangRust::or_pattern => {
            assign_type_to_or_pattern(unit, scopes, pattern, pattern_type);
        }
        LangRust::slice_pattern => {
            assign_type_to_slice_pattern(unit, scopes, pattern, pattern_type);
        }
        LangRust::reference_pattern => {
            assign_type_to_reference_pattern(unit, scopes, pattern, pattern_type);
        }
        LangRust::mut_pattern | LangRust::ref_pattern => {
            if let Some(inner) = pattern.children(unit).first() {
                bind_pattern_types(unit, scopes, inner, pattern_type);
            }
        }
        _ => {
            if let Some(ident) = pattern.query(unit).try_first_ident() {
                assign_type_to_ident(unit, scopes, ident, pattern_type);
            } else {
                for child in pattern.children(unit) {
                    bind_pattern_types(unit, scopes, &child, pattern_type);
                }
            }
        }
    }
}

/// Attach a known type to one binding identifier if it does not already have one.
fn assign_type_to_ident<'tcx>(
    _unit: &CompileUnit<'tcx>,
    scopes: &mut BindCtxt<'tcx>,
    ident: &'tcx llmcc_core::ir::HirIdent<'tcx>,
    ident_type: &'tcx Symbol,
) {
    let default_type = ident_type;

    let symbol = match ident.try_symbol() {
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

    if symbol.kind().is_const() {
        return;
    }

    if symbol.type_of().is_some() && symbol.kind().is_resolved() {
        return;
    }

    if symbol.type_of().is_none() {
        symbol.set_type_of(default_type.id());
    }
}

/// Propagate tuple element types by position.
fn assign_type_to_tuple_pattern<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BindCtxt<'tcx>,
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
                if let Some(element_type) = unit.try_symbol(*type_id) {
                    bind_pattern_types(unit, scopes, &child, element_type);
                }
            }
        }
        element_index += 1;
    }
}

/// Propagate named struct field types into field patterns.
fn assign_type_to_struct_pattern<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BindCtxt<'tcx>,
    pattern: &HirNode<'tcx>,
    _pattern_type: &'tcx Symbol,
) {
    let struct_type_node = match pattern.child_by_field(unit, LangRust::field_type) {
        Some(node) => node,
        None => return,
    };

    let struct_type_ident = match struct_type_node.query(unit).try_first_ident() {
        Some(ident) => ident,
        None => return,
    };

    let struct_symbol = match struct_type_ident.try_symbol() {
        Some(sym) => sym,
        None => return,
    };

    for child in pattern.children(unit) {
        if child.kind_id() == LangRust::field_pattern {
            if let Some(field_name_node) = child.child_by_field(unit, LangRust::field_name) {
                if let Some(field_name_ident) = field_name_node.query(unit).try_first_ident() {
                    if let Some(field_sym) = scopes.lookup_member(
                        struct_symbol,
                        field_name_ident.name,
                        SymKindSet::from_kind(SymKind::Field),
                    ) {
                        let field_type = field_sym
                            .type_of()
                            .and_then(|type_id| unit.try_symbol(type_id));

                        if let Some(inner_pattern) =
                            child.child_by_field(unit, LangRust::field_pattern)
                            && let Some(field_type) = field_type
                        {
                            bind_pattern_types(unit, scopes, &inner_pattern, field_type);
                        } else if let Some(field_type) = field_type {
                            if let Some(binding_sym) = field_name_ident.try_symbol() {
                                if binding_sym.kind() == SymKind::Variable {
                                    binding_sym.set_type_of(field_type.id());
                                } else if let Some(var_sym) = scopes.lookup_symbol(
                                    field_name_ident.name,
                                    SymKindSet::from_kind(SymKind::Variable),
                                ) {
                                    field_name_ident.set_symbol(var_sym);
                                    assign_type_to_ident(
                                        unit,
                                        scopes,
                                        field_name_ident,
                                        field_type,
                                    );
                                }
                            } else if let Some(var_sym) = scopes.lookup_symbol(
                                field_name_ident.name,
                                SymKindSet::from_kind(SymKind::Variable),
                            ) {
                                field_name_ident.set_symbol(var_sym);
                                assign_type_to_ident(unit, scopes, field_name_ident, field_type);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Propagate tuple-struct/variant field types by position.
fn assign_type_to_tuple_struct_pattern<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BindCtxt<'tcx>,
    pattern: &HirNode<'tcx>,
    _pattern_type: &'tcx Symbol,
) {
    let type_node = match pattern.child_by_field(unit, LangRust::field_type) {
        Some(node) => node,
        None => return,
    };

    let type_ident = match type_node.query(unit).try_first_ident() {
        Some(ident) => ident,
        None => return,
    };

    let type_symbol = match type_ident.try_symbol() {
        Some(sym) => sym,
        None => return,
    };

    let nested_types = type_symbol.nested_types();

    let mut element_index = 0;
    for child in pattern.children(unit) {
        if child.field_id() == LangRust::field_type || child.is_trivia() {
            continue;
        }

        if let Some(ref types) = nested_types {
            if let Some(type_id) = types.get(element_index) {
                if let Some(element_type) = unit.try_symbol(*type_id) {
                    bind_pattern_types(unit, scopes, &child, element_type);
                }
            }
        }

        element_index += 1;
    }
}

/// Every alternative in an or-pattern has the same matched type.
fn assign_type_to_or_pattern<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BindCtxt<'tcx>,
    pattern: &HirNode<'tcx>,
    pattern_type: &'tcx Symbol,
) {
    for child in pattern.children(unit) {
        if !child.is_trivia() {
            bind_pattern_types(unit, scopes, &child, pattern_type);
        }
    }
}

/// Slice/array pattern elements share the collection element type.
fn assign_type_to_slice_pattern<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BindCtxt<'tcx>,
    pattern: &HirNode<'tcx>,
    pattern_type: &'tcx Symbol,
) {
    let element_type = match pattern_type.nested_types() {
        Some(types) => match types.first().and_then(|type_id| unit.try_symbol(*type_id)) {
            Some(ty) => ty,
            None => return,
        },
        None => return,
    };

    for child in pattern.children(unit) {
        if child.is_trivia() {
            continue;
        }

        bind_pattern_types(unit, scopes, &child, element_type);
    }
}

/// Reference patterns bind their inner pattern to the referenced type.
fn assign_type_to_reference_pattern<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &mut BindCtxt<'tcx>,
    pattern: &HirNode<'tcx>,
    pattern_type: &'tcx Symbol,
) {
    let inner_type = match pattern_type.nested_types() {
        Some(types) => match types.first().and_then(|type_id| unit.try_symbol(*type_id)) {
            Some(ty) => ty,
            None => pattern_type,
        },
        None => pattern_type,
    };

    if let Some(inner_pattern) = pattern.child_by_field(unit, LangRust::field_pattern) {
        bind_pattern_types(unit, scopes, &inner_pattern, inner_type);
    } else {
        for child in pattern.children(unit) {
            if !child.is_trivia() {
                bind_pattern_types(unit, scopes, &child, inner_type);
            }
        }
    }
}
