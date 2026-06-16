use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::symbol::{SYM_KIND_TYPES, SymKind, SymKindSet, Symbol};
use llmcc_resolver::BindCtxt;

use crate::token::LangRust;

/// Maximum inference recursion depth to prevent stack overflow.
const MAX_INFER_DEPTH: u32 = 16;

/// Infer the best available symbol for a Rust expression or type node.
pub(crate) fn infer_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BindCtxt<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    infer_type_impl(unit, scopes, node, 0)
}

/// Recursive inference with an explicit depth guard for malformed trees.
fn infer_type_impl<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BindCtxt<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    if depth >= MAX_INFER_DEPTH {
        return None;
    }

    match node.kind_id() {
        LangRust::boolean_literal => get_primitive_type(scopes, "bool"),
        LangRust::integer_literal => get_primitive_type(scopes, "i32"),
        LangRust::float_literal => get_primitive_type(scopes, "f64"),
        LangRust::char_literal => get_primitive_type(scopes, "char"),
        LangRust::string_literal => get_primitive_type(scopes, "str"),

        LangRust::primitive_type | LangRust::type_identifier => {
            let ident = node.query(unit).try_first_ident()?;
            if let Some(sym) = ident.try_symbol() {
                if sym.kind().is_defined_type() {
                    return Some(sym);
                }
                if sym.kind() != SymKind::UnresolvedType {
                    return Some(sym);
                }
            }
            if let Some(sym) = scopes.lookup_symbol(ident.name, SYM_KIND_TYPES) {
                return Some(sym);
            }
            ident.try_symbol()
        }

        LangRust::block => infer_block(unit, scopes, node, depth + 1),

        // Generic type with turbofish: Vec::<T>::new()
        LangRust::generic_type_with_turbofish => node
            .child_by_field(unit, LangRust::field_type)
            .and_then(|ty_node| infer_type_impl(unit, scopes, &ty_node, depth + 1)),

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

        LangRust::identifier => {
            let ident = node.query(unit).try_first_ident()?;
            let symbol = ident.try_symbol()?;

            if let Some(type_id) = symbol.type_of() {
                unit.try_symbol(type_id)
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
                    return unit.try_symbol(ret_id);
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
                    unit.try_symbol(type_id)
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

        LangRust::array_type => infer_array_type(unit, scopes, node, depth + 1),
        LangRust::tuple_type => infer_tuple_type(unit, scopes, node, depth + 1),
        LangRust::function_type => infer_function_type(unit, scopes, node, depth + 1),
        LangRust::reference_type => infer_reference_type(unit, scopes, node, depth + 1),
        LangRust::pointer_type => infer_pointer_type(unit, scopes, node, depth + 1),
        LangRust::abstract_type => infer_abstract_type(unit, scopes, node, depth + 1),
        LangRust::bounded_type => infer_bounded_type(unit, scopes, node, depth + 1),
        LangRust::trait_bounds => infer_trait_bounds(unit, scopes, node, depth + 1),

        _ => {
            if let Some(ident) = node.query(unit).try_first_ident() {
                return ident.try_symbol();
            }
            None
        }
    }
}

/// Primitive type symbols live in the initial global scope.
fn get_primitive_type<'tcx>(scopes: &BindCtxt<'tcx>, name: &str) -> Option<&'tcx Symbol> {
    scopes
        .lookup_globals(name, SymKindSet::from_kind(SymKind::Primitive))?
        .last()
        .copied()
}

/// A block expression has the type of its last non-let expression.
fn infer_block<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BindCtxt<'tcx>,
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
    scopes: &BindCtxt<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    infer_from_children(unit, scopes, node, &[LangRust::field_length], depth + 1)
}

/// Infer range expression type: 1..5 -> Range<i32>
fn infer_range_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BindCtxt<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    infer_from_children(unit, scopes, node, &[], depth + 1)
        .or_else(|| get_primitive_type(scopes, "i32"))
}

/// Tuple expressions currently collect element types but do not synthesize a tuple symbol.
fn infer_tuple_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BindCtxt<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    let children = node.children(unit);

    let mut _elem_types = Vec::new();
    for child in children {
        if !child.is_trivia()
            && let Some(elem_type) = infer_type_impl(unit, scopes, &child, depth + 1)
        {
            _elem_types.push(elem_type);
        }
    }

    None
}

/// Infer struct expression type: Struct { ... } -> Struct
fn infer_struct_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BindCtxt<'tcx>,
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
    scopes: &BindCtxt<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    let value_node = node.child_by_field(unit, LangRust::field_value)?;
    let obj_type = infer_type_impl(unit, scopes, &value_node, depth + 1)?;

    let field_node = node.child_by_field(unit, LangRust::field_field)?;
    let field_ident = field_node.query(unit).try_first_ident()?;

    if field_node.kind() == HirKind::Text {
        let field_text = unit.hir_text(&field_node);
        if let Ok(index) = field_text.parse::<usize>() {
            if let Some(nested) = obj_type.nested_types()
                && let Some(elem_id) = nested.get(index)
            {
                return unit.try_symbol(*elem_id);
            }
            return None;
        }
    }

    scopes
        .lookup_member_kind(obj_type, field_ident.name, SymKind::Field)
        .and_then(|field_sym| {
            if let Some(type_id) = field_sym.type_of() {
                unit.try_symbol(type_id)
            } else {
                Some(field_sym)
            }
        })
}

/// Resolve a qualified Rust path through resolver path lookup.
fn infer_scoped_identifier<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BindCtxt<'tcx>,
    node: &HirNode<'tcx>,
    _depth: u32,
) -> Option<&'tcx Symbol> {
    let idents = node.query(unit).identifiers();

    if idents.is_empty() {
        return None;
    }

    let qualified_names: Vec<&str> = idents.iter().map(|i| i.name).collect();

    // `crate::x` is a Rust keyword path, while resolver lookup uses package names.
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

    scopes
        .lookup_path_symbol(lookup_path, SymKindSet::empty())
        .or_else(|| idents.last().and_then(|ident| ident.try_symbol()))
}

/// Infer index expression type: arr[i] -> ElementType
fn infer_index_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BindCtxt<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    let value_node = node.child_by_field(unit, LangRust::field_value)?;
    let obj_type = infer_type_impl(unit, scopes, &value_node, depth + 1)?;

    if let Some(nested) = obj_type.nested_types()
        && let Some(elem_id) = nested.first()
    {
        return unit.try_symbol(*elem_id);
    }

    None
}

/// Infer binary expression type: a + b or a == b
fn infer_binary_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BindCtxt<'tcx>,
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

/// If expressions currently use the consequence block as the best available type.
fn infer_if_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BindCtxt<'tcx>,
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
    scopes: &BindCtxt<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    if let Some(sn) = node.as_scope()
        && let Some(array_ident) = sn.try_ident()
        && let Some(array_symbol) = scopes.lookup_symbol(
            array_ident.name,
            SymKindSet::from_kind(SymKind::CompositeType),
        )
    {
        return Some(array_symbol);
    }

    let elem_node = node.child_by_field(unit, LangRust::field_element)?;
    infer_type_impl(unit, scopes, &elem_node, depth + 1)
}

/// Infer tuple type annotation: (T1, T2, T3)
fn infer_tuple_type<'tcx>(
    _unit: &CompileUnit<'tcx>,
    scopes: &BindCtxt<'tcx>,
    node: &HirNode<'tcx>,
    _depth: u32,
) -> Option<&'tcx Symbol> {
    if let Some(sn) = node.as_scope()
        && let Some(tuple_ident) = sn.try_ident()
        && let Some(tuple_symbol) = scopes.lookup_symbol(
            tuple_ident.name,
            SymKindSet::from_kind(SymKind::CompositeType),
        )
    {
        return Some(tuple_symbol);
    }

    None
}

/// Infer function type annotation: fn(T1, T2) -> RetType or FnOnce() -> RetType
fn infer_function_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BindCtxt<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    // `FnOnce() -> T` is modeled as the trait; bare `fn(T) -> U` uses return type.
    if let Some(trait_node) = node.child_by_field(unit, LangRust::field_trait) {
        return infer_type_impl(unit, scopes, &trait_node, depth + 1);
    }

    if let Some(ret_node) = node.child_by_field(unit, LangRust::field_return_type)
        && let Some(ret_sym) = infer_type_impl(unit, scopes, &ret_node, depth + 1)
    {
        return Some(ret_sym);
    }

    if let Some(params_node) = node.child_by_field(unit, LangRust::field_parameters) {
        for child in params_node.children(unit) {
            if child.is_trivia() {
                continue;
            }
            if let Some(param_type) = infer_type_impl(unit, scopes, &child, depth + 1) {
                return Some(param_type);
            }
        }
    }

    None
}

/// Prefer the outer generic type when known; otherwise use a defined type argument.
fn infer_generic_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BindCtxt<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    let type_node = node.child_by_field(unit, LangRust::field_type)?;
    let outer_type = infer_type_impl(unit, scopes, &type_node, depth + 1);

    if let Some(outer) = outer_type
        && outer.kind().is_defined_type()
    {
        return Some(outer);
    }

    // Unknown standard-library wrappers still need useful dependency targets.
    if let Some(type_args) = node.child_by_field(unit, LangRust::field_type_arguments) {
        for child in type_args.children(unit) {
            if child.is_trivia() || child.kind_id() == LangRust::lifetime {
                continue;
            }
            if let Some(inner_type) = infer_type_impl(unit, scopes, &child, depth + 1) {
                if inner_type.kind().is_defined_type() {
                    return Some(inner_type);
                }
            }
        }
    }

    outer_type
}

/// Infer reference type annotation: &T or &mut T
fn infer_reference_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BindCtxt<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    node.child_by_field(unit, LangRust::field_type)
        .and_then(|type_node| infer_type_impl(unit, scopes, &type_node, depth + 1))
}

/// Infer pointer type annotation: *const T or *mut T
fn infer_pointer_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BindCtxt<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    node.child_by_field(unit, LangRust::field_type)
        .and_then(|type_node| infer_type_impl(unit, scopes, &type_node, depth + 1))
}

/// Infer abstract type (impl Trait): impl Trait or impl for<'a> Trait
fn infer_abstract_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BindCtxt<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    let trait_node = node.child_by_field(unit, LangRust::field_trait)?;
    infer_type_impl(unit, scopes, &trait_node, depth + 1)
}

/// Infer bounded type: 'a + Clone, T: Clone + Debug
/// Returns the first non-lifetime type found
fn infer_bounded_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BindCtxt<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    for child in node.children(unit) {
        if child.kind_id() == LangRust::lifetime {
            continue;
        }
        if child.is_trivia() {
            continue;
        }
        if let Some(sym) = infer_type_impl(unit, scopes, &child, depth + 1) {
            return Some(sym);
        }
    }
    None
}

/// Infer trait bounds: Into<T> + Clone - returns first non-lifetime type
fn infer_trait_bounds<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BindCtxt<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    for child in node.children(unit) {
        if child.kind_id() == LangRust::lifetime || child.is_trivia() {
            continue;
        }
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
    scopes: &BindCtxt<'tcx>,
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
