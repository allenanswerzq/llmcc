use std::cell::Cell;

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::symbol::{SYM_KIND_TYPES, SymKind, SymKindSet, Symbol};
use llmcc_resolver::BinderScopes;

use crate::token::LangCpp;

/// Maximum recursion depth for type inference to prevent exponential blowup
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

/// Infer the type of any C/C++ AST node
#[tracing::instrument(skip_all)]
pub fn infer_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // Depth limit to prevent exponential recursion on complex types
    let _guard = DepthGuard::try_new()?;

    match node.kind_id() {
        // Literal types
        LangCpp::number_literal => get_primitive_type(scopes, "int"),
        LangCpp::char_literal => get_primitive_type(scopes, "char"),
        LangCpp::string_literal | LangCpp::concatenated_string | LangCpp::raw_string_literal => {
            get_primitive_type(scopes, "char")
        }
        LangCpp::r#true | LangCpp::r#false => get_primitive_type(scopes, "bool"),
        LangCpp::null | LangCpp::nullptr => get_primitive_type(scopes, "nullptr_t"),

        // Primitive type
        LangCpp::primitive_type => {
            let type_text = unit.hir_text(node);
            get_primitive_type(scopes, &type_text)
        }

        // Sized type specifier (unsigned int, long long, etc.)
        LangCpp::sized_type_specifier => {
            // For sized types, try to find the base type
            for child in node.children(unit) {
                if child.kind_id() == LangCpp::primitive_type {
                    return infer_type(unit, scopes, &child);
                }
            }
            // Default to int for things like "unsigned"
            get_primitive_type(scopes, "int")
        }

        // Type identifiers
        LangCpp::type_identifier => {
            let ident = node.find_ident(unit)?;
            // First try existing symbol on the identifier
            if let Some(sym) = ident.opt_symbol() {
                if sym.kind().is_defined_type() {
                    return Some(sym);
                }
                if sym.kind() != SymKind::UnresolvedType {
                    return Some(sym);
                }
            }
            // Try to look up by name in scopes
            if let Some(sym) = scopes.lookup_symbol(ident.name, SYM_KIND_TYPES) {
                return Some(sym);
            }
            // Try global lookup
            if let Some(sym) = scopes.lookup_global(ident.name, SYM_KIND_TYPES) {
                return Some(sym);
            }
            ident.opt_symbol()
        }

        // Identifier
        LangCpp::identifier => {
            let ident = node.find_ident(unit)?;
            let symbol = ident.opt_symbol()?;

            if let Some(type_id) = symbol.type_of() {
                unit.opt_get_symbol(type_id)
            } else {
                Some(symbol)
            }
        }

        // Template type: vector<int>, map<string, int>, etc.
        LangCpp::template_type => infer_template_type(unit, scopes, node),

        // Qualified identifier: std::vector, foo::Bar
        LangCpp::qualified_identifier => infer_qualified_identifier(unit, scopes, node),

        // Call expression: func(args)
        LangCpp::call_expression => node
            .child_by_field(unit, LangCpp::field_function)
            .and_then(|func_node| infer_type(unit, scopes, &func_node))
            .and_then(|sym| {
                if sym.kind() == SymKind::Function
                    && let Some(ret_id) = sym.type_of()
                {
                    return unit.opt_get_symbol(ret_id);
                }
                Some(sym)
            }),

        // New expression: new Type()
        LangCpp::new_expression => {
            // Get the first child which is the type being allocated
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

        // Field expression: obj.field or obj->field
        LangCpp::field_expression => infer_field_expression(unit, scopes, node),

        // Subscript expression: arr[i]
        LangCpp::subscript_expression => infer_subscript_expression(unit, scopes, node),

        // Binary expression: a + b, a == b, etc.
        LangCpp::binary_expression => infer_binary_expression(unit, scopes, node),

        // Unary expression: *ptr, &value, -a, !b
        LangCpp::unary_expression | LangCpp::pointer_expression => {
            infer_from_children(unit, scopes, node, &[])
        }

        // Conditional expression: cond ? a : b
        LangCpp::conditional_expression => node
            .child_by_field(unit, LangCpp::field_consequence)
            .and_then(|conseq_node| infer_type(unit, scopes, &conseq_node)),

        // Cast expression: (Type)value
        LangCpp::cast_expression => {
            // First child should be the type
            for child in node.children(unit) {
                if !child.is_trivia() && child.kind() != HirKind::Text {
                    return infer_type(unit, scopes, &child);
                }
            }
            None
        }

        // Compound statement (block)
        LangCpp::compound_statement => infer_block(unit, scopes, node),

        // Parenthesized expression
        LangCpp::parenthesized_expression => infer_from_children(unit, scopes, node, &[]),

        // Lambda expression
        LangCpp::lambda_expression => {
            // Lambdas have an implicit function type; return void by default
            get_primitive_type(scopes, "void")
        }

        // Decltype
        LangCpp::decltype => {
            // decltype(expr) - infer type from the expression
            infer_from_children(unit, scopes, node, &[])
        }

        // Auto type - can't infer without more context
        LangCpp::auto => None,

        // Type descriptor
        LangCpp::type_descriptor => {
            // Type descriptor wraps a type, find the actual type
            for child in node.children(unit) {
                if let Some(sym) = infer_type(unit, scopes, &child) {
                    return Some(sym);
                }
            }
            None
        }

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

/// Check if a node is syntactic noise (punctuation, keywords, etc.)
fn is_syntactic_noise(_unit: &CompileUnit, node: &HirNode) -> bool {
    matches!(node.kind(), HirKind::Text | HirKind::Comment)
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

        if kind_id == LangCpp::return_statement {
            // Return type from return statement
            if let Some(value_node) = child.child_by_field(unit, LangCpp::field_value) {
                return infer_type(unit, scopes, &value_node);
            }
            continue;
        }

        if kind_id == LangCpp::expression_statement {
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

/// Infer type from child nodes, skipping specified kinds
fn infer_from_children<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    skip_kinds: &[u16],
) -> Option<&'tcx Symbol> {
    for child in node.children(unit) {
        if is_syntactic_noise(unit, &child) {
            continue;
        }
        if skip_kinds.contains(&child.kind_id()) {
            continue;
        }
        if let Some(sym) = infer_type(unit, scopes, &child) {
            return Some(sym);
        }
    }
    None
}

/// Infer template type: vector<int>, map<K, V>, etc.
fn infer_template_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // Get the base type (e.g., 'vector' from 'vector<int>')
    if let Some(name_node) = node.child_by_field(unit, LangCpp::field_name) {
        return infer_type(unit, scopes, &name_node);
    }

    // Fall back to first identifier
    if let Some(ident) = node.find_ident(unit) {
        if let Some(sym) = ident.opt_symbol() {
            return Some(sym);
        }
        if let Some(sym) = scopes.lookup_symbol(ident.name, SYM_KIND_TYPES) {
            return Some(sym);
        }
    }

    None
}

/// Infer qualified identifier: std::vector, foo::bar
fn infer_qualified_identifier<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // Get the name field (the final identifier in the chain)
    if let Some(name_node) = node.child_by_field(unit, LangCpp::field_name)
        && let Some(ident) = name_node.find_ident(unit)
        && let Some(sym) = ident.opt_symbol()
    {
        return Some(sym);
    }

    // Try to look up the full qualified name
    let full_name = unit.hir_text(node);
    if let Some(sym) = scopes.lookup_symbol(&full_name, SYM_KIND_TYPES) {
        return Some(sym);
    }

    None
}

/// Infer field expression: obj.field or obj->field
fn infer_field_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    _scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // Try to get the field's symbol directly
    if let Some(field_node) = node.child_by_field(unit, LangCpp::field_field)
        && let Some(field_ident) = field_node.find_ident(unit)
        && let Some(field_sym) = field_ident.opt_symbol()
    {
        // Return the field's type if known
        if let Some(type_id) = field_sym.type_of() {
            return unit.opt_get_symbol(type_id);
        }
        return Some(field_sym);
    }

    None
}

/// Infer subscript expression: arr[i]
fn infer_subscript_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // Get the array/container type
    if let Some(arg_node) = node.child_by_field(unit, LangCpp::field_argument)
        && let Some(container_type) = infer_type(unit, scopes, &arg_node)
    {
        // For arrays, the element type would be in nested_types
        // For now, return the container type
        return Some(container_type);
    }
    None
}

/// Infer binary expression: a + b, a == b, etc.
fn infer_binary_expression<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    // Get the operator
    if let Some(op_node) = node.child_by_field(unit, LangCpp::field_operator) {
        let op_text = unit.hir_text(&op_node);

        // Comparison operators return bool
        if matches!(
            op_text.as_str(),
            "==" | "!=" | "<" | ">" | "<=" | ">=" | "&&" | "||"
        ) {
            return get_primitive_type(scopes, "bool");
        }
    }

    // For arithmetic operators, infer from the left operand
    if let Some(left_node) = node.child_by_field(unit, LangCpp::field_left) {
        return infer_type(unit, scopes, &left_node);
    }

    None
}
