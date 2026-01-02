use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{SYM_KIND_ALL, SYM_KIND_TYPES, SymKind, SymKindSet, Symbol};
use llmcc_resolver::BinderScopes;

use crate::token::LangPython;

/// Infer the type of any AST node
#[tracing::instrument(skip_all)]
pub fn infer_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    match node.kind_id() {
        // Literal types
        LangPython::true_literal | LangPython::false_literal => get_builtin_type(scopes, "bool"),
        LangPython::integer => get_builtin_type(scopes, "int"),
        LangPython::float => get_builtin_type(scopes, "float"),
        LangPython::string | LangPython::concatenated_string => get_builtin_type(scopes, "str"),
        LangPython::none_literal => get_builtin_type(scopes, "None"),

        // Container literals
        LangPython::list | LangPython::list_comprehension => get_builtin_type(scopes, "list"),
        LangPython::dictionary | LangPython::dictionary_comprehension => {
            get_builtin_type(scopes, "dict")
        }
        LangPython::set | LangPython::set_comprehension => get_builtin_type(scopes, "set"),
        LangPython::tuple | LangPython::tuple_expression => get_builtin_type(scopes, "tuple"),

        // Generator expression
        LangPython::generator_expression => get_builtin_type(scopes, "generator"),

        // Identifier - look up the symbol
        LangPython::identifier => {
            let ident = node.find_ident(unit)?;
            let symbol = ident.opt_symbol()?;

            if let Some(type_id) = symbol.type_of() {
                unit.opt_get_symbol(type_id)
            } else if symbol.kind().is_defined_type() {
                Some(symbol)
            } else {
                Some(symbol)
            }
        }

        // Attribute access - try to resolve the attribute's type
        LangPython::attribute => infer_attribute_type(unit, scopes, node),

        // Subscript - returns element type if possible
        LangPython::subscript => infer_subscript_type(unit, scopes, node),

        // Call expression - returns the return type of the function
        LangPython::call => infer_call_type(unit, scopes, node),

        // Binary operator
        LangPython::binary_operator => infer_binary_type(unit, scopes, node),

        // Unary operator
        LangPython::unary_operator => node
            .child_by_field(unit, LangPython::field_argument)
            .and_then(|arg| infer_type(unit, scopes, &arg)),

        // Boolean operators
        LangPython::boolean_operator | LangPython::not_operator => get_builtin_type(scopes, "bool"),

        // Comparison operators
        LangPython::comparison_operator => get_builtin_type(scopes, "bool"),

        // Conditional expression: x if cond else y
        LangPython::conditional_expression => node
            .child_by_field(unit, LangPython::field_consequence)
            .and_then(|c| infer_type(unit, scopes, &c)),

        // Named expression (walrus operator): x := value
        LangPython::named_expression => node
            .child_by_field(unit, LangPython::field_value)
            .and_then(|v| infer_type(unit, scopes, &v)),

        // Await expression - returns awaited type
        LangPython::await_ => node
            .child_by_field(unit, LangPython::field_argument)
            .and_then(|arg| infer_type(unit, scopes, &arg)),

        // Parenthesized expression
        LangPython::parenthesized_expression => {
            infer_from_children(unit, scopes, node)
        }

        // Type annotations
        LangPython::type_ => infer_type_annotation(unit, scopes, node),
        LangPython::generic_type => infer_generic_type(unit, scopes, node),

        _ => {
            // Try to find an identifier in the node
            if let Some(ident) = node.find_ident(unit) {
                return ident.opt_symbol();
            }
            None
        }
    }
}

/// Get a built-in type by name
fn get_builtin_type<'tcx>(scopes: &BinderScopes<'tcx>, name: &str) -> Option<&'tcx Symbol> {
    scopes
        .lookup_globals(name, SymKindSet::from_kind(SymKind::Primitive))?
        .last()
        .copied()
}

/// Infer type from children nodes
fn infer_from_children<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    for &child_id in node.child_ids() {
        let child = unit.hir_node(child_id);
        if let Some(sym) = infer_type(unit, scopes, &child) {
            return Some(sym);
        }
    }
    None
}

/// Infer type of attribute access
fn infer_attribute_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // First infer the object type
    let object_node = node.child_by_field(unit, LangPython::field_object)?;
    let object_type = infer_type(unit, scopes, &object_node)?;

    // Try to get the attribute from the object's scope
    let attr_node = node.child_by_field(unit, LangPython::field_attribute)?;
    let attr_ident = attr_node.as_ident()?;

    // Use scopes to lookup member
    scopes.lookup_member_symbol(object_type, &attr_ident.name, None)
}

/// Infer type of subscript access
fn infer_subscript_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // Get the value being subscripted
    let value_node = node.child_by_field(unit, LangPython::field_value)?;
    let _value_type = infer_type(unit, scopes, &value_node)?;

    // For now, we can't infer element types of generic containers
    // This would require more sophisticated type analysis
    None
}

/// Infer type of function call
fn infer_call_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    let func_node = node.child_by_field(unit, LangPython::field_function)?;
    let func_sym = infer_type(unit, scopes, &func_node)?;

    // If calling a class, the result is an instance of that class
    if func_sym.kind() == SymKind::Struct {
        return Some(func_sym);
    }

    // If calling a function, return its return type
    if func_sym.kind() == SymKind::Function || func_sym.kind() == SymKind::Method {
        if let Some(ret_id) = func_sym.type_of() {
            return unit.opt_get_symbol(ret_id);
        }
    }

    Some(func_sym)
}

/// Infer type of binary operation
fn infer_binary_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // For most binary operations, the result type is the same as the operands
    // This is a simplification - Python has complex rules here
    if let Some(left_node) = node.child_by_field(unit, LangPython::field_left) {
        if let Some(left_type) = infer_type(unit, scopes, &left_node) {
            return Some(left_type);
        }
    }

    if let Some(right_node) = node.child_by_field(unit, LangPython::field_right) {
        if let Some(right_type) = infer_type(unit, scopes, &right_node) {
            return Some(right_type);
        }
    }

    None
}

/// Infer type from type annotation
fn infer_type_annotation<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // Type annotations can be identifiers, generic types, union types, etc.
    if let Some(ident) = node.find_ident(unit) {
        // Try to look up the type
        if let Some(sym) = scopes.lookup_symbol(&ident.name, SYM_KIND_TYPES) {
            return Some(sym);
        }
        // Also try as primitive/builtin
        if let Some(sym) = get_builtin_type(scopes, &ident.name) {
            return Some(sym);
        }
        // Fall back to any symbol (handles imported types)
        if let Some(sym) = scopes.lookup_symbol(&ident.name, SYM_KIND_ALL) {
            return Some(sym);
        }
        // For unresolved types, use the identifier's symbol if available
        if let Some(sym) = ident.opt_symbol() {
            return Some(sym);
        }
    }

    // Recurse into children
    infer_from_children(unit, scopes, node)
}

/// Infer type from generic type annotation (e.g., List[int], Dict[str, int])
fn infer_generic_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // Get the base type (e.g., List, Dict)
    for &child_id in node.child_ids() {
        let child = unit.hir_node(child_id);
        if child.kind_id() == LangPython::identifier {
            if let Some(ident) = child.as_ident() {
                // Try to look up the type
                if let Some(sym) = scopes.lookup_symbol(&ident.name, SYM_KIND_TYPES) {
                    return Some(sym);
                }
                if let Some(sym) = get_builtin_type(scopes, &ident.name) {
                    return Some(sym);
                }
                // Fall back to any symbol (handles imported types like List, Dict, etc.)
                if let Some(sym) = scopes.lookup_symbol(&ident.name, SYM_KIND_ALL) {
                    return Some(sym);
                }
                // For unresolved generic types, return the identifier's symbol if it has one
                if let Some(sym) = ident.opt_symbol() {
                    return Some(sym);
                }
            }
            break;
        }
    }

    None
}
