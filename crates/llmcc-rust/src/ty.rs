use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_resolver::BinderScopes;

use crate::token::LangRust;

/// Infer the type of any AST node
#[tracing::instrument(skip_all)]
pub fn infer_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    match node.kind_id() {
        // Literal types
        LangRust::boolean_literal => get_primitive_type(scopes, "bool"),
        LangRust::integer_literal => get_primitive_type(scopes, "i32"),
        LangRust::float_literal => get_primitive_type(scopes, "f64"),
        LangRust::char_literal => get_primitive_type(scopes, "char"),
        LangRust::string_literal => get_primitive_type(scopes, "str"),

        // Type identifiers
        LangRust::primitive_type | LangRust::type_identifier => {
            let ident = node.find_ident(unit)?;
            ident.opt_symbol()
        }

        // Block expressions
        LangRust::block => infer_block(unit, scopes, node),

        // Generic type with turbofish: Vec::<T>::new()
        LangRust::generic_type_with_turbofish => node
            .child_by_field(unit, LangRust::field_type)
            .and_then(|ty_node| infer_type(unit, scopes, &ty_node)),

        // Array expression: [1, 2, 3] or [1; 5]
        LangRust::array_expression => infer_array_expression(unit, scopes, node),

        // Range expression: 1..5, 1..=5, 1.., ..5, etc.
        LangRust::range_expression => infer_range_expression(unit, scopes, node),

        // Tuple expression: (a, b, c)
        LangRust::tuple_expression => infer_tuple_expression(unit, scopes, node),

        // Struct expression: Struct { field: value }
        LangRust::struct_expression => infer_struct_expression(unit, scopes, node),

        // Field expression: obj.field or obj[index]
        LangRust::field_expression => infer_field_expression(unit, scopes, node),

        // Scoped identifier: module::Type or foo::bar::baz
        LangRust::scoped_identifier => infer_scoped_identifier(unit, scopes, node),

        // Identifier
        LangRust::identifier => {
            let ident = node.find_ident(unit)?;
            ident.opt_symbol().and_then(|sym| {
                if let Some(type_id) = sym.type_of() {
                    unit.opt_get_symbol(type_id)
                } else {
                    Some(sym)
                }
            })
        }

        // Call expression: func(args)
        LangRust::call_expression | LangRust::field_identifier => node
            .child_by_field(unit, LangRust::field_function)
            .and_then(|func_node| infer_type(unit, scopes, &func_node)),

        // Index expression: arr[i]
        LangRust::index_expression => infer_index_expression(unit, scopes, node),

        // Binary expression: a + b, a == b, etc.
        LangRust::binary_expression => infer_binary_expression(unit, scopes, node),

        // Unary expression: -a, !b, *ptr, &ref
        LangRust::unary_expression => node
            .child_by_field(unit, LangRust::field_argument)
            .and_then(|arg_node| infer_type(unit, scopes, &arg_node)),

        // Type cast expression: expr as Type
        LangRust::type_cast_expression => node
            .child_by_field(unit, LangRust::field_type)
            .and_then(|ty_node| infer_type(unit, scopes, &ty_node)),

        // If expression: if cond { ... } else { ... }
        LangRust::if_expression => infer_if_expression(unit, scopes, node),

        // Type nodes
        LangRust::array_type => infer_array_type(unit, scopes, node),
        LangRust::tuple_type => infer_tuple_type(unit, scopes, node),
        LangRust::function_type => infer_function_type(unit, scopes, node),
        LangRust::reference_type => infer_reference_type(unit, scopes, node),
        LangRust::pointer_type => infer_pointer_type(unit, scopes, node),

        _ => {
            // try to find identifier
            if let Some(ident) = node.find_ident(unit) {
                return ident.opt_symbol();
            }
            // search from scopes
            if let Some(ty) = scopes.lookup_symbol(&unit.hir_text(node), SymKind::type_kinds()) {
                return Some(ty);
            }
            None
        }
    }
}

/// Get primitive type by name
fn get_primitive_type<'tcx>(scopes: &BinderScopes<'tcx>, name: &str) -> Option<&'tcx Symbol> {
    scopes
        .lookup_globals(name, vec![SymKind::Primitive])?
        .last()
        .copied()
}

/// Infer block type: type of last expression in block
fn infer_block<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    let children = node.children(unit);
    let last_expr = children.iter().rev().find(|child| !child.is_trivia())?;

    infer_type(unit, scopes, last_expr)
}

/// Infer array expression type: [elem; count] or [elem1, elem2, ...]
fn infer_array_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    let children = node.children(unit);
    let first_expr = children.iter().find(|child| !child.is_trivia())?;

    let elem_type = infer_type(unit, scopes, first_expr)?;

    // For now, return the element type.
    // A full implementation would create a synthetic [T] type
    Some(elem_type)
}

/// Infer range expression type: 1..5 -> Range<i32>
fn infer_range_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    let children = node.children(unit);

    // Try to infer element type from first expression
    let element_type = children
        .iter()
        .find(|child| !child.is_trivia())
        .and_then(|expr| infer_type(unit, scopes, expr))
        .or_else(|| get_primitive_type(scopes, "i32"))?;

    // For now, return element type. Full implementation would create Range<T>
    Some(element_type)
}

/// Infer tuple expression type: (a, b, c) -> (TypeA, TypeB, TypeC)
fn infer_tuple_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    let children = node.children(unit);

    // Collect all non-trivia expressions (tuple elements)
    let mut _elem_types = Vec::new();
    for child in children {
        if !child.is_trivia() {
            if let Some(elem_type) = infer_type(unit, scopes, &child) {
                _elem_types.push(elem_type);
            }
        }
    }

    // For now, return None as we don't have synthetic type creation.
    // Full implementation would create (T1, T2, T3) type
    None
}

/// Infer struct expression type: Struct { ... } -> Struct
fn infer_struct_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // Try to get the name field (explicit struct name)
    if let Some(name_node) = node.child_by_field(unit, LangRust::field_name) {
        return infer_type(unit, scopes, &name_node);
    }

    // Try to get the type field
    if let Some(type_node) = node.child_by_field(unit, LangRust::field_type) {
        return infer_type(unit, scopes, &type_node);
    }

    None
}

/// Infer field expression type: obj.field -> FieldType
fn infer_field_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    let value_node = node.child_by_field(unit, LangRust::field_value)?;
    let obj_type = infer_type(unit, scopes, &value_node)?;

    let field_node = node.child_by_field(unit, LangRust::field_field)?;
    let field_ident = field_node.find_ident(unit)?;

    // Check if field is a numeric literal (tuple indexing)
    if field_node.kind() == HirKind::Text {
        let field_text = unit.hir_text(&field_node);
        if let Ok(index) = field_text.parse::<usize>() {
            // Tuple indexing: get element type from nested_types
            if let Some(nested) = obj_type.nested_types() {
                if let Some(elem_id) = nested.get(index) {
                    return unit.opt_get_symbol(*elem_id);
                }
            }
            return None;
        }
    }

    // Look up field in object's scope
    scopes
        .lookup_member_symbol(obj_type, &field_ident.name, Some(SymKind::Field))
        .and_then(|field_sym| {
            if let Some(type_id) = field_sym.type_of() {
                unit.opt_get_symbol(type_id)
            } else {
                Some(field_sym)
            }
        })
}

/// Infer scoped identifier type: module::Type or foo::bar::baz
fn infer_scoped_identifier<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    let idents = node.collect_idents(unit);

    if idents.is_empty() {
        return None;
    }

    let qualified_names: Vec<&str> = idents.iter().map(|i| i.name.as_str()).collect();

    tracing::trace!("resolving scoped ident {:?}", qualified_names);

    scopes
        .lookup_qualified(&qualified_names, None)?
        .last()
        .copied()
}

/// Infer index expression type: arr[i] -> ElementType
fn infer_index_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    let value_node = node.child_by_field(unit, LangRust::field_value)?;
    let obj_type = infer_type(unit, scopes, &value_node)?;

    // For indexed access, get first nested type
    if let Some(nested) = obj_type.nested_types() {
        if let Some(elem_id) = nested.first() {
            return unit.opt_get_symbol(*elem_id);
        }
    }

    None
}

/// Infer binary expression type: a + b or a == b
fn infer_binary_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    let children = node.children(unit);
    let left_node = children.first()?;

    // Find the operator
    let operator = children.iter().skip(1).find(|child| child.is_operator())?;

    match operator.kind_id() {
        // Comparison operators return bool
        LangRust::Text_EQEQ
        | LangRust::Text_NE
        | LangRust::Text_LT
        | LangRust::Text_GT
        | LangRust::Text_LE
        | LangRust::Text_GE
        | LangRust::Text_AMPAMP
        | LangRust::Text_PIPEPIPE => get_primitive_type(scopes, "bool"),

        // Arithmetic operators return left operand type
        LangRust::Text_PLUS
        | LangRust::Text_MINUS
        | LangRust::Text_STAR
        | LangRust::Text_SLASH
        | LangRust::Text_PERCENT => infer_type(unit, scopes, left_node),

        _ => None,
    }
}

/// Infer if expression type: type of consequence block
fn infer_if_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    if let Some(consequence) = node.child_by_field(unit, LangRust::field_consequence) {
        return infer_block(unit, scopes, &consequence);
    }
    None
}

/// Infer array type annotation: [T; N]
fn infer_array_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    let elem_node = node.child_by_field(unit, LangRust::field_element)?;
    infer_type(unit, scopes, &elem_node)
}

/// Infer tuple type annotation: (T1, T2, T3)
fn infer_tuple_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    let children = node.children(unit);

    let mut _elem_types = Vec::new();
    for child in children {
        if !child.is_trivia() {
            if let Some(elem_type) = infer_type(unit, scopes, &child) {
                _elem_types.push(elem_type);
            }
        }
    }

    // For now return None, full implementation creates (T1, T2, T3) type
    None
}

/// Infer function type annotation: fn(T1, T2) -> RetType
fn infer_function_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    let children = node.children(unit);

    // Last non-trivia child should be the return type
    children
        .iter()
        .rev()
        .find(|child| !child.is_trivia())
        .and_then(|ret_node| infer_type(unit, scopes, ret_node))
}

/// Infer reference type annotation: &T or &mut T
fn infer_reference_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    let children = node.children(unit);

    // First non-trivia child is the referenced type
    children
        .iter()
        .find(|child| !child.is_trivia())
        .and_then(|ref_node| infer_type(unit, scopes, ref_node))
}

/// Infer pointer type annotation: *const T or *mut T
fn infer_pointer_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    let children = node.children(unit);

    // First non-trivia child is the pointed-to type
    children
        .iter()
        .find(|child| !child.is_trivia())
        .and_then(|ptr_node| infer_type(unit, scopes, ptr_node))
}
