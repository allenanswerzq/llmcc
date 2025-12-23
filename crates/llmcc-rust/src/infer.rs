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
        LangRust::scoped_type_identifier => infer_scoped_identifier(unit, scopes, node),

        // Identifier
        LangRust::identifier => {
            let ident = node.find_ident(unit)?;
            let symbol = ident.opt_symbol()?;

            if let Some(type_id) = symbol.type_of() {
                unit.opt_get_symbol(type_id)
            } else {
                Some(symbol)
            }
        }

        // Call expression: func(args)
        LangRust::call_expression | LangRust::field_identifier => node
            .child_by_field(unit, LangRust::field_function)
            .and_then(|func_node| infer_type(unit, scopes, &func_node))
            .and_then(|sym| {
                if sym.kind() == SymKind::Function
                    && let Some(ret_id) = sym.type_of()
                {
                    return unit.opt_get_symbol(ret_id);
                }
                Some(sym)
            }),

        // Index expression: arr[i]
        LangRust::index_expression => infer_index_expression(unit, scopes, node),

        // Binary expression: a + b, a == b, etc.
        LangRust::binary_expression => infer_binary_expression(unit, scopes, node),

        // Reference expression: &value
        LangRust::reference_expression => {
            infer_from_children(unit, scopes, node, &[]).and_then(|sym| {
                if let Some(type_id) = sym.type_of() {
                    unit.opt_get_symbol(type_id)
                } else {
                    Some(sym)
                }
            })
        }

        // Unary expression: -a, !b,
        LangRust::unary_expression => node
            .child_by_field(unit, LangRust::field_argument)
            .and_then(|arg_node| infer_type(unit, scopes, &arg_node))
            .or_else(|| infer_from_children(unit, scopes, node, &[])),

        // Expression statements simply forward their inner expression type
        LangRust::expression_statement => infer_from_children(unit, scopes, node, &[]),

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
        // impl Trait - get the trait from the "trait" field
        LangRust::abstract_type => infer_abstract_type(unit, scopes, node),
        // Bounded type: 'a + Clone, T: Clone + Debug
        LangRust::bounded_type => infer_bounded_type(unit, scopes, node),

        _ => {
            if let Some(ident) = node.find_ident(unit) {
                return ident.opt_symbol();
            }
            // search from scopes
            // if let Some(ty) = scopes.lookup_symbol(&unit.hir_text(node), SymKind::type_kinds()) {
            //     return Some(ty);
            // }
            None
        }
    }
}

/// Get primitive type by name
#[tracing::instrument(skip_all)]
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
    for child in node.children(unit).into_iter().rev() {
        if is_syntactic_noise(unit, &child) {
            continue;
        }

        let kind_id = child.kind_id();

        if kind_id == LangRust::let_declaration {
            continue;
        }

        if kind_id == LangRust::expression_statement {
            if let Some(sym) = infer_from_children(unit, scopes, &child, &[]) {
                return Some(sym);
            }
            continue;
        }

        if let Some(sym) = infer_type(unit, scopes, &child) {
            return Some(sym);
        }
    }

    None
}

/// Infer array expression type: [elem; count] or [elem1, elem2, ...]
fn infer_array_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    infer_from_children(unit, scopes, node, &[LangRust::field_length])
}

/// Infer range expression type: 1..5 -> Range<i32>
fn infer_range_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    infer_from_children(unit, scopes, node, &[]).or_else(|| get_primitive_type(scopes, "i32"))
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
        if !child.is_trivia()
            && let Some(elem_type) = infer_type(unit, scopes, &child)
        {
            _elem_types.push(elem_type);
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
    let type_kinds = SymKind::type_kinds();

    if let Some(name_node) = node.child_by_field(unit, LangRust::field_name)
        && let Some(sym) = infer_type(unit, scopes, &name_node)
        && type_kinds.contains(&sym.kind())
    {
        tracing::trace!("inferring struct type from name node");
        return Some(sym);
    }

    None
}

/// Infer field expression type: obj.field -> FieldType
#[tracing::instrument(skip_all)]
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
            if let Some(nested) = obj_type.nested_types()
                && let Some(elem_id) = nested.get(index)
            {
                return unit.opt_get_symbol(*elem_id);
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
#[tracing::instrument(skip_all)]
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
    if let Some(nested) = obj_type.nested_types()
        && let Some(elem_id) = nested.first()
    {
        return unit.opt_get_symbol(*elem_id);
    }

    None
}

/// Infer binary expression type: a + b or a == b
#[tracing::instrument(skip_all)]
fn infer_binary_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    let children = node.children(unit);
    let left_node = children.first()?;
    let operator = node.child_by_field(unit, LangRust::field_operator)?;
    match operator.kind_id() {
        LangRust::Text_EQEQ
        | LangRust::Text_NE
        | LangRust::Text_LT
        | LangRust::Text_GT
        | LangRust::Text_LE
        | LangRust::Text_GE
        | LangRust::Text_AMPAMP
        | LangRust::Text_PIPEPIPE => get_primitive_type(scopes, "bool"),

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
    // Try to get the CompositeType symbol that was created for this array type
    if let Some(sn) = node.as_scope()
        && let Some(array_ident) = sn.opt_ident()
        && let Some(array_symbol) =
            scopes.lookup_symbol(&array_ident.name, vec![SymKind::CompositeType])
    {
        return Some(array_symbol);
    }

    // Fallback: get element type
    let elem_node = node.child_by_field(unit, LangRust::field_element)?;
    infer_type(unit, scopes, &elem_node)
}

/// Infer tuple type annotation: (T1, T2, T3)
fn infer_tuple_type<'tcx>(
    _unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // Try to get the CompositeType symbol that was created for this tuple type
    if let Some(sn) = node.as_scope()
        && let Some(tuple_ident) = sn.opt_ident()
        && let Some(tuple_symbol) =
            scopes.lookup_symbol(&tuple_ident.name, vec![SymKind::CompositeType])
    {
        return Some(tuple_symbol);
    }

    // Fallback: collect element types but can't return a proper symbol
    // This handles cases where the tuple type wasn't pre-collected
    None
}

/// Infer function type annotation: fn(T1, T2) -> RetType or FnOnce() -> RetType
fn infer_function_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // For function_type, check if there's a trait field (e.g., FnOnce() -> T)
    // If so, return the trait. Otherwise return the return type.
    if let Some(trait_node) = node.child_by_field(unit, LangRust::field_trait) {
        return infer_type(unit, scopes, &trait_node);
    }

    // No trait field, fall back to return type (for fn(T) -> U syntax)
    node.child_by_field(unit, LangRust::field_return_type)
        .and_then(|ret_node| infer_type(unit, scopes, &ret_node))
}

/// Infer reference type annotation: &T or &mut T
fn infer_reference_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // The reference_type has structure: & [mutable_specifier] type
    // We need to get the inner type via the "type" field
    node.child_by_field(unit, LangRust::field_type)
        .and_then(|type_node| infer_type(unit, scopes, &type_node))
}

/// Infer pointer type annotation: *const T or *mut T
fn infer_pointer_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // The pointer_type has structure: * [const|mut] type
    // We need to get the inner type via the "type" field
    node.child_by_field(unit, LangRust::field_type)
        .and_then(|type_node| infer_type(unit, scopes, &type_node))
}

/// Infer abstract type (impl Trait): impl Trait or impl for<'a> Trait
fn infer_abstract_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // The abstract_type has structure: impl [for<'a>] Trait
    // We need to get the trait from the "trait" field
    node.child_by_field(unit, LangRust::field_trait)
        .and_then(|trait_node| infer_type(unit, scopes, &trait_node))
}

/// Infer bounded type: 'a + Clone, T: Clone + Debug
/// Returns the first non-lifetime type found
fn infer_bounded_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // bounded_type contains multiple bounds separated by +
    // We want to find the actual trait type, skipping lifetimes
    for child in node.children(unit) {
        // Skip lifetimes and punctuation
        if child.kind_id() == LangRust::lifetime {
            continue;
        }
        if child.is_trivia() {
            continue;
        }
        // Try to infer type from this child
        if let Some(sym) = infer_type(unit, scopes, &child) {
            return Some(sym);
        }
    }
    None
}

/// Returns true when a node only represents punctuation or whitespace.
fn is_syntactic_noise<'tcx>(unit: &CompileUnit<'tcx>, node: &HirNode<'tcx>) -> bool {
    if node.is_trivia() {
        return true;
    }

    if node.child_count() > 0 {
        return false;
    }

    unit.hir_text(node)
        .chars()
        .all(|ch| ch.is_whitespace() || ch.is_ascii_punctuation())
}

/// Walks the children of `node` (skipping punctuation/whitespace and optional field ids)
/// and returns the first successfully inferred type.
fn infer_from_children<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    skip_fields: &[u16],
) -> Option<&'tcx Symbol> {
    for child in node.children(unit) {
        if skip_fields.contains(&child.field_id()) {
            continue;
        }

        if is_syntactic_noise(unit, &child) {
            continue;
        }

        if let Some(sym) = infer_type(unit, scopes, &child) {
            return Some(sym);
        }
    }

    None
}
