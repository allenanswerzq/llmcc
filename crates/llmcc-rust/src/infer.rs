use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::symbol::{SYM_KIND_TYPES, SymKind, SymKindSet, Symbol};
use llmcc_resolver::BinderScopes;

use crate::token::LangRust;

/// Maximum inference recursion depth to prevent stack overflow.
const MAX_INFER_DEPTH: u32 = 16;

/// Infer the type of any AST node
pub fn infer_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    infer_type_impl(unit, scopes, node, 0)
}

/// Internal implementation with explicit depth tracking.
fn infer_type_impl<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    if depth >= MAX_INFER_DEPTH {
        return None;
    }

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
            // First try existing symbol on the identifier
            if let Some(sym) = ident.opt_symbol() {
                // If it's a concrete type (Struct/Enum/Trait), use it directly
                if sym.kind().is_defined_type() {
                    return Some(sym);
                }
                // If it's UnresolvedType, try to resolve it via scope lookup
                if sym.kind() != SymKind::UnresolvedType {
                    return Some(sym);
                }
            }
            // Try to look up by name in scopes (for unresolved types)
            if let Some(sym) = scopes.lookup_symbol(ident.name, SYM_KIND_TYPES) {
                return Some(sym);
            }
            // Fall back to original symbol if lookup failed
            ident.opt_symbol()
        }

        // Block expressions
        LangRust::block => infer_block(unit, scopes, node, depth + 1),

        // Generic type with turbofish: Vec::<T>::new()
        LangRust::generic_type_with_turbofish => node
            .child_by_field(unit, LangRust::field_type)
            .and_then(|ty_node| infer_type_impl(unit, scopes, &ty_node, depth + 1)),

        // Generic type: Vec<T>, Option<String>, etc.
        LangRust::generic_type => infer_generic_type(unit, scopes, node, depth + 1),

        // Array expression: [1, 2, 3] or [1; 5]
        LangRust::array_expression => infer_array_expression(unit, scopes, node, depth + 1),

        // Range expression: 1..5, 1..=5, 1.., ..5, etc.
        LangRust::range_expression => infer_range_expression(unit, scopes, node, depth + 1),

        // Tuple expression: (a, b, c)
        LangRust::tuple_expression => infer_tuple_expression(unit, scopes, node, depth + 1),

        // Struct expression: Struct { field: value }
        LangRust::struct_expression => infer_struct_expression(unit, scopes, node, depth + 1),

        // Field expression: obj.field or obj[index]
        LangRust::field_expression => infer_field_expression(unit, scopes, node, depth + 1),

        // Scoped identifier: module::Type or foo::bar::baz
        LangRust::scoped_identifier => infer_scoped_identifier(unit, scopes, node, depth + 1),
        LangRust::scoped_type_identifier => infer_scoped_identifier(unit, scopes, node, depth + 1),

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
            .and_then(|func_node| infer_type_impl(unit, scopes, &func_node, depth + 1))
            .and_then(|sym| {
                if sym.kind() == SymKind::Function
                    && let Some(ret_id) = sym.type_of()
                {
                    return unit.opt_get_symbol(ret_id);
                }
                Some(sym)
            }),

        // Index expression: arr[i]
        LangRust::index_expression => infer_index_expression(unit, scopes, node, depth + 1),

        // Binary expression: a + b, a == b, etc.
        LangRust::binary_expression => infer_binary_expression(unit, scopes, node, depth + 1),

        // Reference expression: &value
        LangRust::reference_expression => infer_from_children(unit, scopes, node, &[], depth + 1)
            .and_then(|sym| {
                if let Some(type_id) = sym.type_of() {
                    unit.opt_get_symbol(type_id)
                } else {
                    Some(sym)
                }
            }),

        // Unary expression: -a, !b,
        LangRust::unary_expression => node
            .child_by_field(unit, LangRust::field_argument)
            .and_then(|arg_node| infer_type_impl(unit, scopes, &arg_node, depth + 1))
            .or_else(|| infer_from_children(unit, scopes, node, &[], depth + 1)),

        // Expression statements simply forward their inner expression type
        LangRust::expression_statement => infer_from_children(unit, scopes, node, &[], depth + 1),

        // Type cast expression: expr as Type
        LangRust::type_cast_expression => node
            .child_by_field(unit, LangRust::field_type)
            .and_then(|ty_node| infer_type_impl(unit, scopes, &ty_node, depth + 1)),

        // If expression: if cond { ... } else { ... }
        LangRust::if_expression => infer_if_expression(unit, scopes, node, depth + 1),

        // Type nodes
        LangRust::array_type => infer_array_type(unit, scopes, node, depth + 1),
        LangRust::tuple_type => infer_tuple_type(unit, scopes, node, depth + 1),
        LangRust::function_type => infer_function_type(unit, scopes, node, depth + 1),
        LangRust::reference_type => infer_reference_type(unit, scopes, node, depth + 1),
        LangRust::pointer_type => infer_pointer_type(unit, scopes, node, depth + 1),
        // impl Trait - get the trait from the "trait" field
        LangRust::abstract_type => infer_abstract_type(unit, scopes, node, depth + 1),
        // Bounded type: 'a + Clone, T: Clone + Debug
        LangRust::bounded_type => infer_bounded_type(unit, scopes, node, depth + 1),
        // Trait bounds: Into<T> + Clone - returns first non-lifetime type
        LangRust::trait_bounds => infer_trait_bounds(unit, scopes, node, depth + 1),

        _ => {
            if let Some(ident) = node.find_ident(unit) {
                return ident.opt_symbol();
            }
            None
        }
    }
}

/// Get primitive type by name
fn get_primitive_type<'tcx>(scopes: &BinderScopes<'tcx>, name: &str) -> Option<&'tcx Symbol> {
    scopes
        .lookup_globals(name, SymKindSet::from_kind(SymKind::Primitive))?
        .last()
        .copied()
}

/// Infer block type: type of last expression in block
fn infer_block<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
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
            if let Some(sym) = infer_from_children(unit, scopes, &child, &[], depth + 1) {
                return Some(sym);
            }
            continue;
        }

        if let Some(sym) = infer_type_impl(unit, scopes, &child, depth + 1) {
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
    depth: u32,
) -> Option<&'tcx Symbol> {
    infer_from_children(unit, scopes, node, &[LangRust::field_length], depth + 1)
}

/// Infer range expression type: 1..5 -> Range<i32>
fn infer_range_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    infer_from_children(unit, scopes, node, &[], depth + 1)
        .or_else(|| get_primitive_type(scopes, "i32"))
}

/// Infer tuple expression type: (a, b, c) -> (TypeA, TypeB, TypeC)
fn infer_tuple_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    let children = node.children(unit);

    // Collect all non-trivia expressions (tuple elements)
    let mut _elem_types = Vec::new();
    for child in children {
        if !child.is_trivia()
            && let Some(elem_type) = infer_type_impl(unit, scopes, &child, depth + 1)
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
    depth: u32,
) -> Option<&'tcx Symbol> {
    if let Some(name_node) = node.child_by_field(unit, LangRust::field_name)
        && let Some(sym) = infer_type_impl(unit, scopes, &name_node, depth + 1)
        && SYM_KIND_TYPES.contains(sym.kind())
    {
        return Some(sym);
    }

    None
}

/// Infer field expression type: obj.field -> FieldType
fn infer_field_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    let value_node = node.child_by_field(unit, LangRust::field_value)?;
    let obj_type = infer_type_impl(unit, scopes, &value_node, depth + 1)?;

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
        .lookup_member_symbol(obj_type, field_ident.name, Some(SymKind::Field))
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
    _depth: u32,
) -> Option<&'tcx Symbol> {
    let idents = node.collect_idents(unit);

    if idents.is_empty() {
        return None;
    }

    let qualified_names: Vec<&str> = idents.iter().map(|i| i.name).collect();

    // Handle Rust's "crate" keyword by replacing with actual crate name
    let resolved_path: Vec<&str>;
    let crate_name_owned = unit.unit_meta().package_name.clone();
    let lookup_path = if !qualified_names.is_empty() && qualified_names[0] == "crate" {
        if let Some(ref crate_name) = crate_name_owned {
            resolved_path = std::iter::once(crate_name.as_str())
                .chain(qualified_names[1..].iter().copied())
                .collect();
            &resolved_path[..]
        } else {
            &qualified_names[..]
        }
    } else {
        &qualified_names[..]
    };

    // Use lookup_qualified_symbol to apply same-crate preference for multi-crate scenarios
    scopes.lookup_qualified_symbol(lookup_path, SymKindSet::empty())
}

/// Infer index expression type: arr[i] -> ElementType
fn infer_index_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    let value_node = node.child_by_field(unit, LangRust::field_value)?;
    let obj_type = infer_type_impl(unit, scopes, &value_node, depth + 1)?;

    // For indexed access, get first nested type
    if let Some(nested) = obj_type.nested_types()
        && let Some(elem_id) = nested.first()
    {
        return unit.opt_get_symbol(*elem_id);
    }

    None
}

/// Infer binary expression type: a + b or a == b
fn infer_binary_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
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
        | LangRust::Text_PERCENT => infer_type_impl(unit, scopes, left_node, depth + 1),

        _ => None,
    }
}

/// Infer if expression type: type of consequence block
fn infer_if_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    if let Some(consequence) = node.child_by_field(unit, LangRust::field_consequence) {
        return infer_block(unit, scopes, &consequence, depth + 1);
    }
    None
}

/// Infer array type annotation: [T; N]
fn infer_array_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    // Try to get the CompositeType symbol that was created for this array type
    if let Some(sn) = node.as_scope()
        && let Some(array_ident) = sn.opt_ident()
        && let Some(array_symbol) = scopes.lookup_symbol(
            array_ident.name,
            SymKindSet::from_kind(SymKind::CompositeType),
        )
    {
        return Some(array_symbol);
    }

    // Fallback: get element type
    let elem_node = node.child_by_field(unit, LangRust::field_element)?;
    infer_type_impl(unit, scopes, &elem_node, depth + 1)
}

/// Infer tuple type annotation: (T1, T2, T3)
fn infer_tuple_type<'tcx>(
    _unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    _depth: u32,
) -> Option<&'tcx Symbol> {
    // Try to get the CompositeType symbol that was created for this tuple type
    if let Some(sn) = node.as_scope()
        && let Some(tuple_ident) = sn.opt_ident()
        && let Some(tuple_symbol) = scopes.lookup_symbol(
            tuple_ident.name,
            SymKindSet::from_kind(SymKind::CompositeType),
        )
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
    depth: u32,
) -> Option<&'tcx Symbol> {
    // For function_type, check if there's a trait field (e.g., FnOnce() -> T)
    // If so, return the trait. Otherwise return the return type.
    if let Some(trait_node) = node.child_by_field(unit, LangRust::field_trait) {
        return infer_type_impl(unit, scopes, &trait_node, depth + 1);
    }

    // Try return type first (for fn(T) -> U syntax)
    if let Some(ret_node) = node.child_by_field(unit, LangRust::field_return_type)
        && let Some(ret_sym) = infer_type_impl(unit, scopes, &ret_node, depth + 1)
    {
        return Some(ret_sym);
    }

    // No return type, try to extract type from parameters (for fn(T) without return)
    // Look for the "parameters" field which contains the parameter types
    if let Some(params_node) = node.child_by_field(unit, LangRust::field_parameters) {
        for child in params_node.children(unit) {
            if child.is_trivia() {
                continue;
            }
            // Each child might be a type directly
            if let Some(param_type) = infer_type_impl(unit, scopes, &child, depth + 1) {
                return Some(param_type);
            }
        }
    }

    None
}

/// Infer generic type: Vec<T>, Option<String>, HashMap<K, V>, etc.
///
/// Strategy: From outer to inner, return the first defined type.
/// - For `Vec<Tree>` where Vec is not defined → returns Tree
/// - For `MyStruct<T>` where MyStruct is defined → returns MyStruct
/// - For `Into<Text>` where Into is defined (trait) → returns Into
fn infer_generic_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    // The generic_type has structure: type<type_arguments>
    let type_node = node.child_by_field(unit, LangRust::field_type)?;
    let outer_type = infer_type_impl(unit, scopes, &type_node, depth + 1);

    // If outer type is a defined type (Struct/Enum/Trait), use it
    if let Some(outer) = outer_type
        && outer.kind().is_defined_type()
    {
        return Some(outer);
    }

    // Outer type is not defined (e.g., Vec, Option from std, or TypeParameter).
    // For relationship graphs, we care about contained defined types.
    // Recursively search type arguments for first defined type.
    if let Some(type_args) = node.child_by_field(unit, LangRust::field_type_arguments) {
        for child in type_args.children(unit) {
            if child.is_trivia() || child.kind_id() == LangRust::lifetime {
                continue;
            }
            if let Some(inner_type) = infer_type_impl(unit, scopes, &child, depth + 1) {
                // Use inner type if it's a defined type
                if inner_type.kind().is_defined_type() {
                    return Some(inner_type);
                }
            }
        }
    }

    // Return outer type even if not defined (for unresolved cases)
    outer_type
}

/// Infer reference type annotation: &T or &mut T
fn infer_reference_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    // The reference_type has structure: & [mutable_specifier] type
    // We need to get the inner type via the "type" field
    node.child_by_field(unit, LangRust::field_type)
        .and_then(|type_node| infer_type_impl(unit, scopes, &type_node, depth + 1))
}

/// Infer pointer type annotation: *const T or *mut T
fn infer_pointer_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    // The pointer_type has structure: * [const|mut] type
    // We need to get the inner type via the "type" field
    node.child_by_field(unit, LangRust::field_type)
        .and_then(|type_node| infer_type_impl(unit, scopes, &type_node, depth + 1))
}

/// Infer abstract type (impl Trait): impl Trait or impl for<'a> Trait
fn infer_abstract_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    // The abstract_type has structure: impl [for<'a>] Trait
    // Get the trait from the "trait" field and let infer_type handle generics
    let trait_node = node.child_by_field(unit, LangRust::field_trait)?;
    infer_type_impl(unit, scopes, &trait_node, depth + 1)
}

/// Infer bounded type: 'a + Clone, T: Clone + Debug
/// Returns the first non-lifetime type found
fn infer_bounded_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
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
        if let Some(sym) = infer_type_impl(unit, scopes, &child, depth + 1) {
            return Some(sym);
        }
    }
    None
}

/// Infer trait bounds: Into<T> + Clone - returns first non-lifetime type
fn infer_trait_bounds<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    // trait_bounds contains type children - get the first non-lifetime one
    for child in node.children(unit) {
        // Skip lifetimes and punctuation
        if child.kind_id() == LangRust::lifetime || child.is_trivia() {
            continue;
        }
        // Let infer_type handle all type kinds including generic_type
        if let Some(sym) = infer_type_impl(unit, scopes, &child, depth + 1) {
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
    depth: u32,
) -> Option<&'tcx Symbol> {
    for child in node.children(unit) {
        if skip_fields.contains(&child.field_id()) {
            continue;
        }

        if is_syntactic_noise(unit, &child) {
            continue;
        }

        if let Some(sym) = infer_type_impl(unit, scopes, &child, depth + 1) {
            return Some(sym);
        }
    }

    None
}
