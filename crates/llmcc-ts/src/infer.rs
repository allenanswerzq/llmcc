use std::cell::Cell;

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::symbol::{SYM_KIND_TYPES, SymKind, SymKindSet, Symbol};
use llmcc_resolver::BinderScopes;

use crate::token::LangTypeScript;

/// Maximum recursion depth for type inference to prevent exponential blowup
/// on complex TypeScript types (like zod's 4500-line schemas.ts)
const MAX_INFER_DEPTH: u32 = 16;

thread_local! {
    static INFER_DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Guard that increments depth on creation and decrements on drop
struct DepthGuard;

impl DepthGuard {
    fn try_new() -> Option<Self> {
        INFER_DEPTH.with(|depth| {
            let current = depth.get();
            if current >= MAX_INFER_DEPTH {
                None
            } else {
                depth.set(current + 1);
                Some(DepthGuard)
            }
        })
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        INFER_DEPTH.with(|depth| {
            depth.set(depth.get().saturating_sub(1));
        });
    }
}

/// Infer the type of any TypeScript AST node
#[tracing::instrument(skip_all)]
pub fn infer_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // Depth limit to prevent exponential recursion on complex types
    let _guard = DepthGuard::try_new()?;

    match node.kind_id() {
        // Literal types - check text content
        LangTypeScript::number => get_primitive_type(scopes, "number"),
        LangTypeScript::string => get_primitive_type(scopes, "string"),
        LangTypeScript::regex => get_primitive_type(scopes, "RegExp"),
        LangTypeScript::template_string => get_primitive_type(scopes, "string"),

        // Predefined types (string, number, boolean, etc.)
        LangTypeScript::predefined_type => {
            let type_text = unit.hir_text(node);
            get_primitive_type(scopes, &type_text)
        }

        // Type identifiers
        LangTypeScript::type_identifier => {
            let ident = node.find_ident(unit)?;
            // First try existing symbol on the identifier
            if let Some(sym) = ident.opt_symbol() {
                // If it's a concrete type (Struct/Trait/Enum), use it directly
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
            // Try global lookup for cross-file imports
            if let Some(sym) = scopes.lookup_global(ident.name, SYM_KIND_TYPES) {
                return Some(sym);
            }
            // Fall back to original symbol if lookup failed
            ident.opt_symbol()
        }

        // Nested type identifier: Models.User, Namespace.Type
        LangTypeScript::nested_type_identifier => infer_nested_type_identifier(unit, scopes, node),

        // Identifier
        LangTypeScript::identifier => {
            let ident = node.find_ident(unit)?;
            let symbol = ident.opt_symbol()?;

            if let Some(type_id) = symbol.type_of() {
                unit.opt_get_symbol(type_id)
            } else {
                Some(symbol)
            }
        }

        // Property identifier
        LangTypeScript::property_identifier => {
            let ident = node.find_ident(unit)?;
            ident.opt_symbol()
        }

        // Statement block
        LangTypeScript::statement_block => infer_block(unit, scopes, node),

        // Generic type: Array<T>, Promise<R>, Map<K, V>, etc.
        LangTypeScript::generic_type => infer_generic_type(unit, scopes, node),

        // Array type: T[]
        LangTypeScript::array_type => infer_array_type(unit, scopes, node),

        // Tuple type: [T1, T2, T3]
        LangTypeScript::tuple_type => infer_tuple_type(unit, scopes, node),

        // Union type: T | U
        LangTypeScript::union_type => infer_union_type(unit, scopes, node),

        // Intersection type: T & U
        LangTypeScript::intersection_type => infer_intersection_type(unit, scopes, node),

        // Function type: (a: T) => R
        LangTypeScript::function_type => infer_function_type(unit, scopes, node),

        // Object type: { name: string; age: number }
        LangTypeScript::object_type => infer_object_type(unit, scopes, node),

        // Literal type: "hello" | 42 | true
        LangTypeScript::literal_type => infer_literal_type(unit, scopes, node),

        // Type annotation: : Type
        LangTypeScript::type_annotation => node
            .child_by_field(unit, LangTypeScript::field_type)
            .and_then(|ty_node| infer_type(unit, scopes, &ty_node))
            .or_else(|| infer_from_children(unit, scopes, node, &[])),

        // Call expression: func(args)
        LangTypeScript::call_expression => node
            .child_by_field(unit, LangTypeScript::field_function)
            .and_then(|func_node| infer_type(unit, scopes, &func_node))
            .and_then(|sym| {
                if sym.kind() == SymKind::Function
                    && let Some(ret_id) = sym.type_of()
                {
                    return unit.opt_get_symbol(ret_id);
                }
                Some(sym)
            }),

        // New expression: new Class()
        LangTypeScript::new_expression => {
            // Get the first child which is typically the constructor/class name
            for child in node.children(unit) {
                if !child.is_trivia()
                    && child.kind() != HirKind::Text
                    && let Some(sym) = infer_type(unit, scopes, &child)
                {
                    return Some(sym);
                }
            }
            None
        }

        // Member expression: obj.prop or obj["prop"]
        LangTypeScript::member_expression => infer_member_expression(unit, scopes, node),

        // Subscript expression: arr[i]
        LangTypeScript::subscript_expression => infer_subscript_expression(unit, scopes, node),

        // Binary expression: a + b, a === b, etc.
        LangTypeScript::binary_expression => infer_binary_expression(unit, scopes, node),

        // Unary expression: -a, !b, typeof x
        LangTypeScript::unary_expression => {
            // Get the operand from children
            infer_from_children(unit, scopes, node, &[])
        }

        // Ternary expression: cond ? a : b
        LangTypeScript::ternary_expression => node
            .child_by_field(unit, LangTypeScript::field_consequence)
            .and_then(|conseq_node| infer_type(unit, scopes, &conseq_node)),

        // Await expression: await promise
        LangTypeScript::await_expression => infer_await_expression(unit, scopes, node),

        // As expression (type cast): expr as Type
        LangTypeScript::as_expression => node
            .child_by_field(unit, LangTypeScript::field_type)
            .and_then(|ty_node| infer_type(unit, scopes, &ty_node)),

        // Type assertion: <Type>expr
        LangTypeScript::type_assertion => node
            .child_by_field(unit, LangTypeScript::field_type)
            .and_then(|ty_node| infer_type(unit, scopes, &ty_node)),

        // Satisfies expression: expr satisfies Type
        LangTypeScript::satisfies_expression => node
            .child_by_field(unit, LangTypeScript::field_type)
            .and_then(|ty_node| infer_type(unit, scopes, &ty_node)),

        // Arrow function: (params) => body
        LangTypeScript::arrow_function => infer_arrow_function(unit, scopes, node),

        // Function expression: function(params) { body }
        LangTypeScript::function_expression => infer_function_expression(unit, scopes, node),

        // Object: { key: value }
        LangTypeScript::object => infer_from_children(unit, scopes, node, &[]),

        // Array: [elem1, elem2, ...]
        LangTypeScript::array => infer_array_expression(unit, scopes, node),

        // Conditional type: T extends U ? X : Y
        LangTypeScript::conditional_type => infer_conditional_type(unit, scopes, node),

        // Infer type: infer T
        LangTypeScript::infer_type => None, // Returns the inferred type parameter

        // This type: this
        LangTypeScript::this_type => scopes
            .lookup_symbol("this", SymKindSet::from_kind(SymKind::Variable))
            .and_then(|sym| {
                if let Some(type_id) = sym.type_of() {
                    unit.opt_get_symbol(type_id)
                } else {
                    Some(sym)
                }
            }),

        _ => {
            if let Some(ident) = node.find_ident(unit) {
                return ident.opt_symbol();
            }
            None
        }
    }
}

/// Get primitive type by name
#[tracing::instrument(skip_all)]
fn get_primitive_type<'tcx>(scopes: &BinderScopes<'tcx>, name: &str) -> Option<&'tcx Symbol> {
    scopes
        .lookup_globals(name, SymKindSet::from_kind(SymKind::Primitive))?
        .last()
        .copied()
}

/// Infer nested type identifier: Models.User, A.B.C.MyClass, etc.
/// Resolves qualified type paths using lookup_qualified.
/// Handles arbitrary nesting depth (e.g., A.B.C.D.MyClass).
#[tracing::instrument(skip_all)]
fn infer_nested_type_identifier<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // Collect all identifiers in the path (e.g., ["Api", "V1", "Models", "User"])
    let idents = node.collect_idents(unit);
    if idents.is_empty() {
        return None;
    }

    let qualified_names: Vec<&str> = idents.iter().map(|i| i.name).collect();
    tracing::trace!("resolving nested type identifier {:?}", qualified_names);

    // Use lookup_qualified to resolve the full path
    scopes
        .lookup_qualified(&qualified_names, SYM_KIND_TYPES)?
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

        // Skip variable declarations
        if kind_id == LangTypeScript::variable_declaration
            || kind_id == LangTypeScript::lexical_declaration
        {
            continue;
        }

        // For return statements, get the returned value's type
        if kind_id == LangTypeScript::return_statement {
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

/// Infer generic type: Array<T>, Promise<R>, Map<K, V>, etc.
fn infer_generic_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // The generic_type has structure: type<type_arguments>
    let type_node = node.child_by_field(unit, LangTypeScript::field_name)?;
    let outer_type = infer_type(unit, scopes, &type_node);

    // Check if outer type is a wrapper type that should be unwrapped
    let outer_name = unit.hir_text(&type_node);
    let is_wrapper_type = matches!(
        outer_name.as_str(),
        "Promise"
            | "Array"
            | "ReadonlyArray"
            | "Set"
            | "Map"
            | "WeakMap"
            | "WeakSet"
            | "AsyncGenerator"
            | "Generator"
            | "AsyncIterable"
            | "Iterable"
            | "AsyncIterator"
            | "Iterator"
    );

    // If outer type is a defined type (Struct/Trait) and not a wrapper, use it
    if let Some(outer) = outer_type
        && outer.kind().is_defined_type()
        && !is_wrapper_type
    {
        return Some(outer);
    }

    // For wrapper types or undefined outer types, extract the inner type argument
    // This is important for architecture graphs where Promise<User> should show User
    if is_wrapper_type {
        // For wrapper types, always try to extract and return the inner type
        // First try via field lookup
        if let Some(type_args) = node.child_by_field(unit, LangTypeScript::field_type_arguments) {
            for child in type_args.children(unit) {
                if child.is_trivia() {
                    continue;
                }
                if let Some(inner_type) = infer_type(unit, scopes, &child) {
                    return Some(inner_type);
                }
            }
        }
        // Fallback: iterate all children to find type arguments
        for child in node.children(unit) {
            if child.kind_id() == LangTypeScript::type_arguments {
                for inner_child in child.children(unit) {
                    if inner_child.is_trivia() {
                        continue;
                    }
                    if let Some(inner_type) = infer_type(unit, scopes, &inner_child) {
                        return Some(inner_type);
                    }
                }
            }
        }
    } else if let Some(type_args) = node.child_by_field(unit, LangTypeScript::field_type_arguments)
    {
        // For non-wrapper types, only use inner type if it's a defined type
        for child in type_args.children(unit) {
            if child.is_trivia() {
                continue;
            }
            if let Some(inner_type) = infer_type(unit, scopes, &child)
                && inner_type.kind().is_defined_type()
            {
                return Some(inner_type);
            }
        }
    }

    // Return outer type even if not defined (for unresolved cases)
    outer_type
}

/// Infer array type: T[] or Array<T>
fn infer_array_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
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

    // Fallback: get element type from first child (for T[] syntax)
    let children = node.children(unit);
    for child in children {
        if !child.is_trivia()
            && let Some(elem_type) = infer_type(unit, scopes, &child)
        {
            return Some(elem_type);
        }
    }

    None
}

/// Infer tuple type: [T1, T2, T3]
fn infer_tuple_type<'tcx>(
    _unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
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

    None
}

/// Infer union type: T | U - returns first non-primitive type if possible
fn infer_union_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // For union types, try to find a defined type
    for child in node.children(unit) {
        if child.is_trivia() {
            continue;
        }
        if let Some(sym) = infer_type(unit, scopes, &child)
            && sym.kind().is_defined_type()
        {
            return Some(sym);
        }
    }

    // If no defined type found, return first type
    infer_from_children(unit, scopes, node, &[])
}

/// Infer intersection type: T & U
fn infer_intersection_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // For intersection types, return first type
    infer_from_children(unit, scopes, node, &[])
}

/// Infer function type: (a: T) => R
fn infer_function_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // Try return type first
    if let Some(ret_node) = node.child_by_field(unit, LangTypeScript::field_return_type)
        && let Some(ret_sym) = infer_type(unit, scopes, &ret_node)
    {
        return Some(ret_sym);
    }

    None
}

/// Infer object type: { name: string; age: number }
fn infer_object_type<'tcx>(
    _unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // Try to get the anonymous type symbol
    if let Some(sn) = node.as_scope()
        && let Some(obj_ident) = sn.opt_ident()
        && let Some(obj_symbol) = scopes.lookup_symbol(
            obj_ident.name,
            SymKindSet::from_kind(SymKind::CompositeType),
        )
    {
        return Some(obj_symbol);
    }

    None
}

/// Infer literal type: "hello" | 42 | true
fn infer_literal_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // Get the underlying literal and infer its primitive type
    infer_from_children(unit, scopes, node, &[])
}

/// Infer member expression: obj.prop
fn infer_member_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    let obj_node = node.child_by_field(unit, LangTypeScript::field_object)?;
    let obj_type = infer_type(unit, scopes, &obj_node)?;

    let prop_node = node.child_by_field(unit, LangTypeScript::field_property)?;
    let prop_ident = prop_node.find_ident(unit)?;

    // Look up property in object's scope
    scopes
        .lookup_member_symbol(obj_type, prop_ident.name, Some(SymKind::Field))
        .and_then(|field_sym| {
            if let Some(type_id) = field_sym.type_of() {
                unit.opt_get_symbol(type_id)
            } else {
                Some(field_sym)
            }
        })
}

/// Infer subscript expression: arr[i]
fn infer_subscript_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    let obj_node = node.child_by_field(unit, LangTypeScript::field_object)?;
    let obj_type = infer_type(unit, scopes, &obj_node)?;

    // For indexed access, get first nested type
    if let Some(nested) = obj_type.nested_types()
        && let Some(elem_id) = nested.first()
    {
        return unit.opt_get_symbol(*elem_id);
    }

    None
}

/// Infer binary expression type
fn infer_binary_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    let left_node = node.child_by_field(unit, LangTypeScript::field_left)?;

    // Get operator text from children (binary_expression has left, operator, right)
    let children = node.children(unit);
    let op_text = children
        .iter()
        .find(|c| c.kind() == HirKind::Text)
        .map(|op| unit.hir_text(op))
        .unwrap_or_default();

    match op_text.as_str() {
        // Comparison operators return boolean
        "==" | "===" | "!=" | "!==" | "<" | ">" | "<=" | ">=" | "in" | "instanceof" => {
            get_primitive_type(scopes, "boolean")
        }
        // Logical operators return boolean
        "&&" | "||" => get_primitive_type(scopes, "boolean"),
        // Arithmetic operators preserve left operand type
        "+" | "-" | "*" | "/" | "%" | "**" => infer_type(unit, scopes, &left_node),
        // Bitwise operators return number
        "&" | "|" | "^" | "<<" | ">>" | ">>>" => get_primitive_type(scopes, "number"),
        // Nullish coalescing
        "??" => infer_type(unit, scopes, &left_node),
        _ => None,
    }
}

/// Infer await expression: get the resolved type from Promise<T>
fn infer_await_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    let inner_node = node.child_by_field(unit, LangTypeScript::field_value)?;
    let promise_type = infer_type(unit, scopes, &inner_node)?;

    // For Promise<T>, get T from nested_types
    if let Some(nested) = promise_type.nested_types()
        && let Some(resolved_id) = nested.first()
    {
        return unit.opt_get_symbol(*resolved_id);
    }

    Some(promise_type)
}

/// Infer arrow function return type
fn infer_arrow_function<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // Check for explicit return type annotation
    if let Some(ret_node) = node.child_by_field(unit, LangTypeScript::field_return_type)
        && let Some(ret_sym) = infer_type(unit, scopes, &ret_node)
    {
        return Some(ret_sym);
    }

    // Try to infer from body
    if let Some(body_node) = node.child_by_field(unit, LangTypeScript::field_body) {
        return infer_type(unit, scopes, &body_node);
    }

    None
}

/// Infer function expression return type
fn infer_function_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // Check for explicit return type annotation
    if let Some(ret_node) = node.child_by_field(unit, LangTypeScript::field_return_type)
        && let Some(ret_sym) = infer_type(unit, scopes, &ret_node)
    {
        return Some(ret_sym);
    }

    // Try to infer from body
    if let Some(body_node) = node.child_by_field(unit, LangTypeScript::field_body) {
        return infer_block(unit, scopes, &body_node);
    }

    None
}

/// Infer array expression type: [elem1, elem2, ...]
fn infer_array_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // Infer from first element
    for child in node.children(unit) {
        if !child.is_trivia()
            && child.kind() != HirKind::Text
            && let Some(elem_type) = infer_type(unit, scopes, &child)
        {
            return Some(elem_type);
        }
    }
    None
}

/// Infer conditional type: T extends U ? X : Y
fn infer_conditional_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // Return the consequence type (X in T extends U ? X : Y)
    if let Some(conseq) = node.child_by_field(unit, LangTypeScript::field_consequence) {
        return infer_type(unit, scopes, &conseq);
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
