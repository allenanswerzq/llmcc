//! Type inference for Python AST nodes.
//!
//! Python is dynamically typed, but we can still infer types in many cases:
//! - Literals have known types (int, str, bool, etc.)
//! - Type annotations provide explicit types
//! - Class instantiation returns the class type
//! - Function calls return the function's annotated return type (if any)
//! - Attribute access can be resolved through the object's type

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{SYM_KIND_TYPES, SymKind, SymKindSet, Symbol};
use llmcc_resolver::BinderScopes;

use crate::token::LangPython;

/// Maximum inference recursion depth to prevent stack overflow.
const MAX_INFER_DEPTH: u32 = 16;

/// Infer the type of a Python AST node.
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
        LangPython::integer => get_primitive_type(scopes, "int"),
        LangPython::float => get_primitive_type(scopes, "float"),
        LangPython::string | LangPython::concatenated_string => get_primitive_type(scopes, "str"),
        LangPython::none => get_primitive_type(scopes, "None"),

        // Collection types - try to infer element types
        LangPython::list => infer_list_type(unit, scopes, node, depth + 1),
        LangPython::tuple => infer_tuple_type(unit, scopes, node, depth + 1),
        LangPython::dictionary => get_primitive_type(scopes, "dict"),
        LangPython::set => get_primitive_type(scopes, "set"),

        // Identifiers - look up their type
        LangPython::identifier => {
            let ident = node.find_ident(unit)?;
            // First try existing symbol on the identifier
            if let Some(sym) = ident.opt_symbol() {
                // If it's a concrete type (Struct/class), use it directly
                if sym.kind().is_defined_type() {
                    return Some(sym);
                }
                // Return the type_of if set, otherwise the symbol itself
                if let Some(type_id) = sym.type_of() {
                    return unit.opt_get_symbol(type_id);
                }
                return Some(sym);
            }
            // Try to look up by name in scopes
            scopes.lookup_symbol(ident.name, SYM_KIND_TYPES)
        }

        // Call expression: func(args) or Class(args)
        LangPython::call => infer_call_type(unit, scopes, node, depth + 1),

        // Attribute access: obj.attr
        LangPython::attribute => infer_attribute_type(unit, scopes, node, depth + 1),

        // Subscript: obj[key]
        LangPython::subscript => infer_subscript_type(unit, scopes, node, depth + 1),

        // Binary operations
        LangPython::binary_operator => infer_binary_type(unit, scopes, node, depth + 1),

        // Unary operations
        LangPython::unary_operator | LangPython::not_operator => {
            infer_unary_type(unit, scopes, node, depth + 1)
        }

        // Comparison operations return bool
        LangPython::comparison_operator | LangPython::boolean_operator => {
            get_primitive_type(scopes, "bool")
        }

        // Conditional expression: x if cond else y
        LangPython::conditional_expression => {
            infer_conditional_type(unit, scopes, node, depth + 1)
        }

        // Lambda: lambda params: body
        LangPython::lambda => {
            // Lambdas are callable
            get_primitive_type(scopes, "callable")
        }

        // Comprehensions return their collection type
        LangPython::list_comprehension => get_primitive_type(scopes, "list"),
        LangPython::dictionary_comprehension => get_primitive_type(scopes, "dict"),
        LangPython::set_comprehension => get_primitive_type(scopes, "set"),
        LangPython::generator_expression => get_primitive_type(scopes, "iter"),

        // Parenthesized expression: (expr)
        LangPython::parenthesized_expression => {
            if let Some(child) = node.children(unit).first() {
                infer_type_impl(unit, scopes, child, depth + 1)
            } else {
                None
            }
        }

        // Named expression: x := value
        LangPython::named_expression => {
            if let Some(value_node) = node.child_by_field(unit, LangPython::field_value) {
                infer_type_impl(unit, scopes, &value_node, depth + 1)
            } else {
                None
            }
        }

        _ => {
            // Try to find an identifier in the node
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

/// Infer list type, trying to get element type from first element
fn infer_list_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    // Get the list primitive type
    let list_sym = get_primitive_type(scopes, "list")?;

    // Try to infer element type from first element
    for child in node.children(unit) {
        if child.is_trivia() {
            continue;
        }
        if let Some(elem_type) = infer_type_impl(unit, scopes, &child, depth + 1) {
            // If we could track nested types, we'd add elem_type here
            // For now, just return list
            let _ = elem_type; // suppress unused warning
        }
        break;
    }

    Some(list_sym)
}

/// Infer tuple type
fn infer_tuple_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    let tuple_sym = get_primitive_type(scopes, "tuple")?;

    // Could collect element types for structural typing
    for child in node.children(unit) {
        if child.is_trivia() {
            continue;
        }
        let _ = infer_type_impl(unit, scopes, &child, depth + 1);
    }

    Some(tuple_sym)
}

/// Infer type from a call expression
fn infer_call_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    let func_node = node.child_by_field(unit, LangPython::field_function)?;

    // Get the callable's symbol
    let callable_sym = infer_type_impl(unit, scopes, &func_node, depth + 1)?;

    match callable_sym.kind() {
        // Calling a class returns an instance of that class
        SymKind::Struct => Some(callable_sym),

        // Calling a function returns its return type
        SymKind::Function | SymKind::Method => {
            if let Some(return_type_id) = callable_sym.type_of() {
                unit.opt_get_symbol(return_type_id)
            } else {
                None
            }
        }

        // Calling a type alias might return an instance
        SymKind::TypeAlias => {
            if let Some(aliased_id) = callable_sym.type_of() {
                unit.opt_get_symbol(aliased_id)
            } else {
                Some(callable_sym)
            }
        }

        _ => Some(callable_sym),
    }
}

/// Infer type from attribute access
fn infer_attribute_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    let object_node = node.child_by_field(unit, LangPython::field_object)?;
    let attr_node = node.child_by_field(unit, LangPython::field_attribute)?;
    let attr_ident = attr_node.as_ident()?;

    // If attribute already has a symbol, use it
    if let Some(attr_sym) = attr_ident.opt_symbol() {
        if let Some(type_id) = attr_sym.type_of() {
            return unit.opt_get_symbol(type_id);
        }
        return Some(attr_sym);
    }

    // Get the object's type
    let obj_type = infer_type_impl(unit, scopes, &object_node, depth + 1)?;

    // Look up the attribute in the object's type scope
    scopes.lookup_member_symbol(obj_type, attr_ident.name, None)
}

/// Infer type from subscript access (e.g., list[0], dict["key"])
fn infer_subscript_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    let value_node = node.child_by_field(unit, LangPython::field_value)?;
    let container_type = infer_type_impl(unit, scopes, &value_node, depth + 1)?;

    // Try to get element type from container's nested_types
    if let Some(nested) = container_type.nested_types()
        && !nested.is_empty()
    {
        return unit.opt_get_symbol(nested[0]);
    }

    // Fall back to the container type itself
    Some(container_type)
}

/// Infer type from binary operations
fn infer_binary_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    // For most binary ops, return the type of the left operand
    // This is a simplification - Python has complex numeric coercion rules
    if let Some(left_node) = node.child_by_field(unit, LangPython::field_left) {
        return infer_type_impl(unit, scopes, &left_node, depth + 1);
    }
    None
}

/// Infer type from unary operations
fn infer_unary_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    // 'not' always returns bool
    if node.kind_id() == LangPython::not_operator {
        return get_primitive_type(scopes, "bool");
    }

    // For other unary ops (-x, ~x), return the argument's type
    if let Some(arg_node) = node.child_by_field(unit, LangPython::field_argument) {
        return infer_type_impl(unit, scopes, &arg_node, depth + 1);
    }

    // Try children as fallback
    for child in node.children(unit) {
        if !child.is_trivia() {
            if let Some(ty) = infer_type_impl(unit, scopes, &child, depth + 1) {
                return Some(ty);
            }
        }
    }

    None
}

/// Infer type from conditional expression (x if cond else y)
fn infer_conditional_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    // Try to get the type from the 'consequence' (if-true) branch
    if let Some(consequence) = node.child_by_field(unit, LangPython::field_consequence) {
        return infer_type_impl(unit, scopes, &consequence, depth + 1);
    }

    // Fall back to alternative branch
    if let Some(alternative) = node.child_by_field(unit, LangPython::field_alternative) {
        return infer_type_impl(unit, scopes, &alternative, depth + 1);
    }

    None
}
