use std::vec;

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirKind, HirNode, HirScope};
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_resolver::{BinderScopes, ResolverOption};

use crate::token::AstVisitorRust;
use crate::token::LangRust;
use crate::util::{parse_crate_name, parse_file_name, parse_module_name};
use llmcc_core::lang_def::LanguageTrait;

#[derive(Copy, Clone, PartialEq, Eq)]
enum BinaryOperatorOutcome {
    ReturnsBool,
    ReturnsLeftOperand,
}

const BINARY_OPERATOR_TOKENS: &[(u16, BinaryOperatorOutcome)] = &[
    // "=="
    (LangRust::Text_EQEQ, BinaryOperatorOutcome::ReturnsBool),
    // "!="
    (LangRust::Text_NE, BinaryOperatorOutcome::ReturnsBool),
    // "<"
    (LangRust::Text_LT, BinaryOperatorOutcome::ReturnsBool),
    // ">"
    (LangRust::Text_GT, BinaryOperatorOutcome::ReturnsBool),
    // "<="
    (LangRust::Text_LE, BinaryOperatorOutcome::ReturnsBool),
    // ">="
    (LangRust::Text_GE, BinaryOperatorOutcome::ReturnsBool),
    // "&&"
    (LangRust::Text_AMPAMP, BinaryOperatorOutcome::ReturnsBool),
    // "||"
    (LangRust::Text_PIPEPIPE, BinaryOperatorOutcome::ReturnsBool),
    // "+"
    (
        LangRust::Text_PLUS,
        BinaryOperatorOutcome::ReturnsLeftOperand,
    ),
    // "-"
    (
        LangRust::Text_MINUS,
        BinaryOperatorOutcome::ReturnsLeftOperand,
    ),
    // "*"
    (
        LangRust::Text_STAR,
        BinaryOperatorOutcome::ReturnsLeftOperand,
    ),
    // "/"
    (
        LangRust::Text_SLASH,
        BinaryOperatorOutcome::ReturnsLeftOperand,
    ),
    // "%"
    (
        LangRust::Text_PERCENT,
        BinaryOperatorOutcome::ReturnsLeftOperand,
    ),
];

/// Visitor for resolving symbol bindings and establishing relationships.
#[derive(Debug)]
pub struct BinderVisitor<'tcx> {
    phantom: std::marker::PhantomData<&'tcx ()>,
}

impl<'tcx> BinderVisitor<'tcx> {
    /// Creates a new visitor; typically called once per file.
    fn new() -> Self {
        Self {
            phantom: std::marker::PhantomData,
        }
    }

    fn initialize(&self, node: &HirNode<'tcx>, scopes: &mut BinderScopes<'tcx>) {
        let primitives = [
            "i32", "i64", "i16", "i8", "i128", "isize", "u32", "u64", "u16", "u8", "u128", "usize",
            "f32", "f64", "bool", "char", "str",
        ];
        for prim in primitives {
            scopes.lookup_or_insert_global(prim, node, SymKind::Primitive);
        }
    }

    /// Returns true for identifier-like HIR kinds (plain, scoped, type IDs, etc.).
    ///
    /// Example: both `foo` and `module::foo` nodes are treated as identifiers.
    fn is_identifier_kind(kind_id: u16) -> bool {
        matches!(
            kind_id,
            LangRust::identifier
                | LangRust::scoped_identifier
                | LangRust::field_identifier
                | LangRust::type_identifier
        )
    }

    /// Finds (or reuses) the symbol declared in a specific field.
    ///
    /// Example: when visiting `struct Foo { value: i32 }` we call
    /// `symbol_from_field(.., LangRust::field_name)` to get the `value` symbol so
    /// the binder can attach types/dependencies to it.
    fn symbol_from_field(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &BinderScopes<'tcx>,
        field_id: u16,
    ) -> Option<&'tcx Symbol> {
        if let Some(ident) = node.as_ident() {
            if let Some(existing) = scopes.lookup_symbol(&ident.name) {
                return Some(existing);
            }
            Some(ident.symbol())
        } else {
            let ident = node.child_identifier_by_field(*unit, field_id)?;
            if let Some(existing) = scopes.lookup_symbol(&ident.name) {
                return Some(existing);
            }
            Some(ident.symbol())
        }
    }

    /// Extracts a human-readable identifier from the given node.
    ///
    /// Example: for `Foo::bar` this returns `"bar"`, ignoring the path prefix.
    fn identifier_name(unit: &CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<String> {
        if let Some(ident) = node.as_ident() {
            return Some(Self::normalize_identifier(&ident.name));
        }
        if let Some(ident) = node.find_identifier(*unit) {
            return Some(Self::normalize_identifier(&ident.name));
        }
        if node.kind_id() == LangRust::super_token {
            return Some("super".to_string());
        }
        if node.kind_id() == LangRust::crate_token {
            return Some("crate".to_string());
        }
        None
    }

    /// Normalizes a fully qualified name by returning only the last component.
    fn normalize_identifier(name: &str) -> String {
        name.rsplit("::").next().unwrap_or(name).to_string()
    }

    /// Returns the first child node; handy for wrappers like `(expr)` or `await`.
    fn first_child_node(unit: &CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<HirNode<'tcx>> {
        let child_id = node.children().first()?;
        Some(unit.hir_node(*child_id))
    }

    /// Looks up a callable symbol (function or macro) by name.
    ///
    /// Example: when resolving `foo()` we call this with `"foo"` to see if the
    /// current scope stack already has a matching function/macro symbol.
    fn lookup_callable_symbol(
        &self,
        _unit: &CompileUnit<'tcx>,
        scopes: &BinderScopes<'tcx>,
        name: &str,
    ) -> Option<&'tcx Symbol> {
        if let Some(symbol) = scopes.lookup_symbol_with(
            name,
            Some(vec![SymKind::Function, SymKind::Macro]),
            None,
            None,
        ) {
            return Some(symbol);
        }
        None
    }

    /// Resolves an arbitrary expression down to the callable symbol it ultimately refers to.
    ///
    /// Strategy overview:
    /// 1. Treat direct identifiers (plain or qualified) as callable lookups in the active scope stack.
    /// 2. For field expressions, first check whether the field name resolves to a callable in the current scope
    ///    (useful for associated functions). If not, infer the receiver type and ask `BinderScopes` for the
    ///    matching member symbol so that method calls like `value.foo()` bind correctly.
    /// 3. Transparently unwrap syntactic sugar such as references, calls, awaits, tries, or parentheses so that
    ///    chains like `foo()?`, `foo().await`, or `(foo)()` still resolve to the base callable.
    ///
    ///    The function only returns symbols that can be invoked (functions or macros); non-callable expressions
    ///    bubble up as `None`.
    fn resolve_expression_symbol(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        caller: Option<&Symbol>,
    ) -> Option<&'tcx Symbol> {
        // Inspect the syntax kind so resolution can specialize per expression form.
        match node.kind_id() {
            kind if Self::is_identifier_kind(kind) => {
                if kind == LangRust::scoped_identifier {
                    return Self::resolve_scoped_identifier_symbol(unit, node, scopes, caller);
                }
                // Extract the identifier text (dropping any path prefixes).
                let name = Self::identifier_name(unit, node)?;
                // Look up the callable directly in the current scope chain.
                self.lookup_callable_symbol(unit, scopes, &name)
            }
            kind if kind == LangRust::field_expression => {
                // Grab the AST node that names the field portion of the access.
                let field = node.child_by_field(*unit, LangRust::field_field)?;
                let name = Self::identifier_name(unit, &field)?;
                // Attempt to resolve `Type::field` style associated function before assuming a method.
                if let Some(symbol) = self.lookup_callable_symbol(unit, scopes, &name) {
                    return Some(symbol);
                }

                // Resolve the receiver expression so we know which type owns the field.
                let value = node.child_by_field(*unit, LangRust::field_value)?;
                if let Some(obj_type) = Self::infer_type_from_expr(unit, &value, scopes) {
                    // If the type exposes a callable member with this name, treat it as a method.
                    if let Some(method) =
                        scopes.lookup_member_symbol(obj_type, &name, Some(SymKind::Function))
                    {
                        return Some(method);
                    }
                }
                None
            }
            kind if kind == LangRust::reference_expression => {
                // Strip the reference operator and resolve the underlying expression.
                let value = node.child_by_field(*unit, LangRust::field_value)?;
                self.resolve_expression_symbol(unit, &value, scopes, caller)
            }
            kind if kind == LangRust::call_expression => {
                // Look at the callable expression being invoked.
                let inner = node.child_by_field(*unit, LangRust::field_function)?;
                self.resolve_expression_symbol(unit, &inner, scopes, caller)
            }
            kind if kind == LangRust::await_expression
                || kind == LangRust::try_expression
                || kind == LangRust::parenthesized_expression =>
            {
                // Dig into the wrapped expression (await, try, or parentheses).
                let child = Self::first_child_node(unit, node)?;
                self.resolve_expression_symbol(unit, &child, scopes, caller)
            }
            _ => {
                // Fall back to treating the node as an identifier-like value.
                let name = Self::identifier_name(unit, node)?;
                self.lookup_callable_symbol(unit, scopes, &name)
            }
        }
    }

    /// Resolves `macro_invocation` nodes to their macro symbol (e.g., `log!`).
    fn resolve_macro_symbol(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
    ) -> Option<&'tcx Symbol> {
        let macro_node = node.child_by_field(*unit, LangRust::field_macro)?;
        if macro_node.kind_id() == LangRust::scoped_identifier {
            return Self::resolve_scoped_identifier_symbol(unit, &macro_node, scopes, None);
        }
        let name = Self::identifier_name(unit, &macro_node)?;
        self.lookup_callable_symbol(unit, scopes, &name)
    }

    /// Returns the callable symbol referenced by a `call_expression`.
    ///
    /// Example: for `Foo::new()` it resolves the `Foo::new` function symbol.
    fn resolve_call_target(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        caller: Option<&Symbol>,
    ) -> Option<&'tcx Symbol> {
        let function = node.child_by_field(*unit, LangRust::field_function)?;
        self.resolve_expression_symbol(unit, &function, scopes, caller)
    }

    /// Resolves a type node (like `Vec<Foo>`) into the `Foo`/`Vec` symbols.
    fn resolve_type_from_node(
        unit: &CompileUnit<'tcx>,
        type_node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
    ) -> Option<&'tcx Symbol> {
        let ident = type_node.find_identifier(*unit)?;

        if let Some(existing) = scopes.lookup_symbol_with(
            &ident.name,
            Some(vec![
                SymKind::Struct,
                SymKind::Enum,
                SymKind::Trait,
                SymKind::TypeAlias,
                SymKind::TypeParameter,
                SymKind::Primitive,
                SymKind::UnresolvedType,
            ]),
            None,
            None,
        ) {
            return Some(existing);
        }

        scopes.lookup_or_insert_global(&ident.name, type_node, SymKind::UnresolvedType)
    }

    /// Records a symbol's declared type and dependency on that type.
    fn link_symbol_with_type(symbol: &Symbol, ty: &Symbol) {
        if symbol.type_of().is_none() {
            symbol.set_type_of(ty.id());
        }
        symbol.add_dependency(ty);
    }

    /// Helper that walks all identifier leaves inside a type expression.
    fn visit_type_identifiers<F>(unit: &CompileUnit<'tcx>, node: &HirNode<'tcx>, f: &mut F)
    where
        F: FnMut(String),
    {
        if let Some(ident) = node.as_ident() {
            f(Self::normalize_identifier(&ident.name));
        }
        for child_id in node.children() {
            let child = unit.hir_node(*child_id);
            Self::visit_type_identifiers(unit, &child, f);
        }
    }

    fn link_type_references(
        unit: &CompileUnit<'tcx>,
        type_node: &HirNode<'tcx>,
        scopes: &BinderScopes<'tcx>,
        symbol: &Symbol,
        owner: Option<&Symbol>,
    ) {
        let mut visit = |name: String| {
            if let Some(target) = scopes.lookup_symbol(&name) {
                symbol.add_dependency(target);
                if let Some(owner) = owner {
                    owner.add_dependency(target);
                }
            }
        };
        Self::visit_type_identifiers(unit, type_node, &mut visit);
    }

    /// Reads the `type` child (if present) and associates it with the symbol.
    ///
    /// Used by parameters, fields, consts, statics, etc.
    fn set_symbol_type_from_field(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        symbol: &Symbol,
        owner: Option<&Symbol>,
        field_id: u16,
    ) {
        if let Some(type_node) = node.child_by_field(*unit, field_id) {
            if let Some(ty) = Self::resolve_type_from_node(unit, &type_node, scopes) {
                Self::link_symbol_with_type(symbol, ty);
                if let Some(owner) = owner {
                    owner.add_dependency(ty);
                }
            }
            Self::link_type_references(unit, &type_node, scopes, symbol, owner);
        }
    }

    /// Pushes the scope represented by `sn`, recursing when the HIR already points
    /// at an existing nested scope (e.g., structs/impls store their own scope nodes).
    fn push_scope_node(scopes: &mut BinderScopes<'tcx>, sn: &'tcx HirScope<'tcx>) {
        if sn.opt_ident().is_some() {
            scopes.push_scope_recursive(sn.scope().id());
        } else {
            scopes.push_scope(sn.scope().id());
        }
    }

    fn primitive_type(
        scopes: &mut BinderScopes<'tcx>,
        node: &HirNode<'tcx>,
        primitive: &str,
    ) -> Option<&'tcx Symbol> {
        scopes.lookup_or_insert_global(primitive, node, SymKind::Primitive)
    }

    /// Infers primitive types for literal nodes (e.g., `42` ⇒ `i32`).
    fn infer_literal_kind(
        kind_id: u16,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
    ) -> Option<&'tcx Symbol> {
        if kind_id == LangRust::integer_literal {
            return Self::primitive_type(scopes, node, "i32");
        }
        if kind_id == LangRust::float_literal {
            return Self::primitive_type(scopes, node, "f64");
        }
        if kind_id == LangRust::string_literal {
            return Self::primitive_type(scopes, node, "str");
        }
        if kind_id == LangRust::boolean_literal {
            return Self::primitive_type(scopes, node, "bool");
        }
        if kind_id == LangRust::char_literal {
            return Self::primitive_type(scopes, node, "char");
        }
        None
    }

    fn binary_operator_outcome_by_kind(kind_id: u16) -> Option<BinaryOperatorOutcome> {
        BINARY_OPERATOR_TOKENS
            .iter()
            .find(|(token_id, _)| *token_id == kind_id)
            .map(|(_, outcome)| *outcome)
    }

    fn binary_operator_outcome_by_text(text: &str) -> Option<BinaryOperatorOutcome> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }

        BINARY_OPERATOR_TOKENS
            .iter()
            .find_map(|(token_id, outcome)| {
                LangRust::token_str(*token_id).and_then(|token_text| {
                    if token_text == trimmed {
                        Some(*outcome)
                    } else {
                        None
                    }
                })
            })
    }

    fn binary_operator_type(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        left_child: &HirNode<'tcx>,
        outcome: BinaryOperatorOutcome,
    ) -> Option<&'tcx Symbol> {
        match outcome {
            BinaryOperatorOutcome::ReturnsBool => Self::primitive_type(scopes, node, "bool"),
            BinaryOperatorOutcome::ReturnsLeftOperand => {
                Self::infer_type_from_expr(unit, left_child, scopes)
            }
        }
    }

    /// Handles struct literal expressions (e.g., `Foo { .. }`).
    fn infer_struct_expression_type(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
    ) -> Option<&'tcx Symbol> {
        node.child_by_field(*unit, LangRust::field_name)
            .or_else(|| node.child_by_field(*unit, LangRust::field_type))
            .and_then(|ty| Self::resolve_type_from_node(unit, &ty, scopes))
    }

    /// Infers types for call expressions (e.g., `Foo::new()`).
    fn infer_call_expression_type(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
    ) -> Option<&'tcx Symbol> {
        node.child_by_field(*unit, LangRust::field_function)
            .and_then(|func| Self::infer_type_from_expr(unit, &func, scopes))
    }

    /// Infers the type of `if` expressions (based on branches).
    fn infer_if_expression_type(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
    ) -> Option<&'tcx Symbol> {
        node.child_by_field(*unit, LangRust::field_consequence)
            .and_then(|consequence| Self::infer_block_type(unit, &consequence, scopes))
    }

    /// Handles unary operators like `*ptr` or `&value`.
    fn infer_unary_expression_type(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
    ) -> Option<&'tcx Symbol> {
        node.child_by_field(*unit, LangRust::field_argument)
            .and_then(|operand| Self::infer_type_from_expr(unit, &operand, scopes))
    }

    /// Handles arithmetic/logical operators (e.g., `a + b`).
    ///
    /// This function attempts to determine the result type of a binary expression.
    fn infer_binary_operator_type(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
    ) -> Option<&'tcx Symbol> {
        let children = node.children_nodes(unit);
        if children.is_empty() {
            return None;
        }

        // Strategy 1: Look for a child node that matches a known binary operator kind
        if let Some(outcome) = children
            .iter()
            .find_map(|child| Self::binary_operator_outcome_by_kind(child.kind_id()))
        {
            // The first child is assumed to be the left operand
            let left_child = children.first()?;
            return Self::binary_operator_type(unit, node, scopes, left_child, outcome);
        }

        // Strategy 2: Check the text between the first and second child (e.g. " + ")
        if children.len() >= 2 {
            let left = &children[0];
            let right = &children[1];
            let start = left.end_byte();
            let end = right.start_byte();

            // Ensure there is space between nodes to contain an operator
            if start < end {
                let operator = unit.get_text(start, end);
                if let Some(outcome) = Self::binary_operator_outcome_by_text(operator.as_str()) {
                    return Self::binary_operator_type(unit, node, scopes, left, outcome);
                }
            }
        }

        None
    }

    /// Infers the type for a field access expression like `foo.bar`.
    ///
    /// We first infer the type of the receiver (`foo` in `foo.bar`). Once we know the owner symbol, we query its member scope
    /// for the requested field symbol. If present, the field's declared type is returned so that downstream logic
    /// can understand `foo.bar` as the value of that type. Missing metadata (no receiver type, field, or type
    /// reference) results in `None`, signalling an unresolved expression.
    fn infer_field_expression_type(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
    ) -> Option<&'tcx Symbol> {
        // Locate the receiver portion of the field expression (`foo` in `foo.bar`).
        let value_node = node.child_by_field(*unit, LangRust::field_value)?;
        // Fetch the AST node that represents the field name.
        let field_node = node.child_by_field(*unit, LangRust::field_field)?;
        let field_ident = field_node.as_ident()?;

        // Infer the type symbol associated with the receiver expression.
        let obj_type_symbol = Self::infer_type_from_expr(unit, &value_node, scopes)?;
        // Query the receiver's scope for the field symbol so we can read its metadata.
        let field_symbol = scopes
            .lookup_member_symbol(obj_type_symbol, &field_ident.name, Some(SymKind::Field))
            .or_else(|| {
                scopes.lookup_member_symbol(
                    obj_type_symbol,
                    &field_ident.name,
                    Some(SymKind::Function),
                )
            })?;
        // Extract the type ID stored on the field symbol.
        let field_type_id = field_symbol.type_of()?;
        // Resolve the type identifier into the corresponding symbol in the compile unit.
        let ty = unit.opt_get_symbol(field_type_id)?;

        if Self::is_self_type(unit, ty) {
            return Some(obj_type_symbol);
        }
        Some(ty)
    }

    /// Handles string/char literal nodes.
    fn infer_text_literal_type(
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
    ) -> Option<&'tcx Symbol> {
        let text = node.as_text()?;
        let value = text.text.as_str();
        if value.chars().all(|c| c.is_ascii_digit()) {
            return Self::primitive_type(scopes, node, "i32");
        }
        if value == "true" || value == "false" {
            return Self::primitive_type(scopes, node, "bool");
        }
        if value.starts_with('"') {
            return Self::primitive_type(scopes, node, "str");
        }
        if value.contains('.') && value.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return Self::primitive_type(scopes, node, "f64");
        }
        None
    }

    /// Resolves identifiers to their known types.
    fn infer_identifier_type(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
    ) -> Option<&'tcx Symbol> {
        let ident = node.as_ident()?;
        let symbol = scopes.lookup_symbol(&ident.name)?;
        match symbol.type_of() {
            Some(type_id) => unit.opt_get_symbol(type_id),
            None => Some(symbol),
        }
    }

    /// Returns the first meaningful child expression (skipping trivia).
    fn infer_first_non_trivia_child(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
    ) -> Option<&'tcx Symbol> {
        for child_id in node.children() {
            let child = unit.hir_node(*child_id);
            if !matches!(child.kind(), HirKind::Text | HirKind::Comment)
                && let Some(ty) = Self::infer_type_from_expr(unit, &child, scopes)
            {
                return Some(ty);
            }
        }
        None
    }

    /// Recurses for internal nodes that wrap actual expressions.
    fn infer_internal_node_type(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
    ) -> Option<&'tcx Symbol> {
        if let Some(ty) = Self::infer_binary_operator_type(unit, node, scopes) {
            return Some(ty);
        }
        Self::infer_first_non_trivia_child(unit, node, scopes)
    }

    /// Returns the type symbol for an expression node
    /// Main entry for inferring the type of an expression node.
    fn infer_type_from_expr(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
    ) -> Option<&'tcx Symbol> {
        let kind_id = node.kind_id();

        if let Some(literal_ty) = Self::infer_literal_kind(kind_id, node, scopes) {
            return Some(literal_ty);
        }

        match kind_id {
            kind if kind == LangRust::scoped_identifier => {
                if let Some(sym) = Self::resolve_scoped_identifier_symbol(unit, node, scopes, None)
                {
                    if let Some(type_id) = sym.type_of()
                        && let Some(ty) = unit.opt_get_symbol(type_id)
                    {
                        if Self::is_self_type(unit, ty)
                            && let Some(parent_scope_id) = sym.parent_scope()
                        {
                            let parent_scope = unit.get_scope(parent_scope_id);
                            if let Some(parent_sym) = parent_scope.symbol() {
                                return Some(parent_sym);
                            }
                        }
                        return Some(ty);
                    }
                    return Some(sym);
                }
            }
            kind if kind == LangRust::struct_expression => {
                if let Some(ty) = Self::infer_struct_expression_type(unit, node, scopes) {
                    return Some(ty);
                }
            }
            kind if kind == LangRust::call_expression => {
                if let Some(ty) = Self::infer_call_expression_type(unit, node, scopes) {
                    return Some(ty);
                }
            }
            kind if kind == LangRust::if_expression => {
                if let Some(ty) = Self::infer_if_expression_type(unit, node, scopes) {
                    return Some(ty);
                }
            }
            kind if kind == LangRust::block => {
                return Self::infer_block_type(unit, node, scopes);
            }
            kind if kind == LangRust::unary_expression => {
                if let Some(ty) = Self::infer_unary_expression_type(unit, node, scopes) {
                    return Some(ty);
                }
            }
            kind if kind == LangRust::binary_expression => {
                if let Some(ty) = Self::infer_binary_operator_type(unit, node, scopes) {
                    return Some(ty);
                }
            }
            kind if kind == LangRust::field_expression => {
                // p.x
                if let Some(ty) = Self::infer_field_expression_type(unit, node, scopes) {
                    return Some(ty);
                }
            }
            _ => {}
        }

        match node.kind() {
            HirKind::Identifier => Self::infer_identifier_type(unit, node, scopes),
            HirKind::Internal => Self::infer_internal_node_type(unit, node, scopes),
            HirKind::Text => Self::infer_text_literal_type(node, scopes),
            _ => Self::infer_first_non_trivia_child(unit, node, scopes),
        }
    }

    fn infer_block_type(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
    ) -> Option<&'tcx Symbol> {
        // Get the last child that isn't whitespace/comments
        let last_expr = node.children().iter().rev().find(|child_id| {
            let child = unit.hir_node(**child_id);
            !matches!(child.kind(), HirKind::Text | HirKind::Comment)
        });

        if let Some(last_id) = last_expr {
            let last_node = unit.hir_node(*last_id);
            Self::infer_type_from_expr(unit, &last_node, scopes)
        } else {
            None
        }
    }

    /// Assign inferred type to pattern (for let bindings, parameters, etc.)
    #[allow(clippy::only_used_in_recursion)]
    fn assign_type_to_pattern(
        unit: &CompileUnit<'tcx>,
        pattern: &HirNode<'tcx>,
        ty: &'tcx Symbol,
        scopes: &mut BinderScopes<'tcx>,
    ) {
        match pattern.kind() {
            HirKind::Identifier => {
                if let Some(ident) = pattern.as_ident() {
                    ident.symbol().set_type_of(ty.id());
                    ident.symbol().add_dependency(ty);
                }
            }
            _ => {
                // For complex patterns (tuple patterns, struct patterns), visit all identifiers
                for child_id in pattern.children() {
                    let child = unit.hir_node(*child_id);
                    Self::assign_type_to_pattern(unit, &child, ty, scopes);
                }
            }
        }
    }

    fn link_pattern_type_references(
        unit: &CompileUnit<'tcx>,
        pattern: &HirNode<'tcx>,
        type_node: &HirNode<'tcx>,
        scopes: &BinderScopes<'tcx>,
        owner: Option<&Symbol>,
    ) {
        match pattern.kind() {
            HirKind::Identifier => {
                if let Some(ident) = pattern.as_ident() {
                    Self::link_type_references(unit, type_node, scopes, ident.symbol(), owner);
                }
            }
            _ => {
                for child_id in pattern.children() {
                    let child = unit.hir_node(*child_id);
                    Self::link_pattern_type_references(unit, &child, type_node, scopes, owner);
                }
            }
        }
    }

    /// Resolves the `crate` keyword to the crate root symbol.
    fn resolve_crate_root(scopes: &BinderScopes<'tcx>) -> Option<&'tcx Symbol> {
        scopes.scopes().iter().into_iter().find_map(|s| {
            if let Some(sym) = s.symbol()
                && sym.kind() == SymKind::Crate
            {
                return Some(sym);
            }
            None
        })
    }

    /// Resolves the `super` keyword relative to a given anchor symbol or the current scope.
    ///
    /// This function traverses the scope stack to find the parent module of the context
    /// defined by `anchor`.
    ///
    /// - If `anchor` is `Some(sym)`, it starts searching from the scope associated with `sym`.
    ///   This is used for paths like `foo::super`, where we want the parent of `foo`.
    /// - If `anchor` is `None`, it starts searching from the current scope.
    ///   This is used for paths like `super::foo`, where we want the parent of the current module.
    ///
    /// The function looks for "module-like" symbols (Module, File, Crate, Namespace) in the stack.
    /// It finds the index of the base scope (either the anchor's scope or the current module scope),
    /// and then searches upwards (reverse) to find the *next* module-like symbol, which represents
    /// the parent module.
    fn resolve_super_relative_to(
        scopes: &BinderScopes<'tcx>,
        anchor: Option<&Symbol>,
    ) -> Option<&'tcx Symbol> {
        let stack = scopes.scopes().iter();

        // Determine the starting point in the scope stack.
        let base_index = if let Some(anchor_sym) = anchor {
            // Case 1: Anchor provided (e.g., `foo::super`).
            // Find the index of the scope that corresponds to `foo`.
            let anchor_scope_id = anchor_sym.scope()?;
            stack.iter().rposition(|s| s.id() == anchor_scope_id)?
        } else {
            // Case 2: No anchor (e.g., `super::foo`).
            // Find the index of the nearest module-like scope in the current stack.
            // This represents the "current module".
            stack.iter().enumerate().rev().find_map(|(i, s)| {
                if let Some(sym) = s.symbol()
                    && matches!(
                        sym.kind(),
                        SymKind::Module | SymKind::File | SymKind::Crate | SymKind::Namespace
                    )
                {
                    return Some(i);
                }
                None
            })?
        };

        // Search upwards from the base index to find the parent module.
        // `take(base_index)` restricts the search to scopes *above* the base.
        // `rev()` iterates from the nearest parent upwards.
        stack.iter().take(base_index).rev().find_map(|s| {
            if let Some(sym) = s.symbol()
                && matches!(
                    sym.kind(),
                    SymKind::Module | SymKind::File | SymKind::Crate | SymKind::Namespace
                )
            {
                return Some(sym);
            }
            None
        })
    }

    /// Resolves a scoped identifier (e.g. `Foo::bar`) to a symbol.
    fn resolve_scoped_identifier_symbol(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        caller: Option<&Symbol>,
    ) -> Option<&'tcx Symbol> {
        let children = node.children_nodes(unit);
        let non_trivia: Vec<_> = children
            .iter()
            .filter(|n| !matches!(n.kind(), HirKind::Text | HirKind::Comment))
            .collect();

        if non_trivia.len() < 2 {
            return None;
        }

        let path_node = non_trivia.first()?;
        let name_node = non_trivia.last()?;
        let name = Self::identifier_name(unit, name_node)?;

        let path_symbol = if path_node.kind_id() == LangRust::scoped_identifier {
            Self::resolve_scoped_identifier_symbol(unit, path_node, scopes, caller)?
        } else if path_node.kind_id() == LangRust::super_token {
            Self::resolve_super_relative_to(scopes, None)?
        } else if path_node.kind_id() == LangRust::crate_token {
            Self::resolve_crate_root(scopes)?
        } else {
            let path_name = Self::identifier_name(unit, path_node)?;
            let sym = scopes.lookup_symbol(&path_name)?;
            if let Some(c) = caller {
                c.add_dependency(sym);
            }
            sym
        };

        if name_node.kind_id() == LangRust::super_token {
            return Self::resolve_super_relative_to(scopes, Some(path_symbol));
        }

        scopes.lookup_member_symbol(path_symbol, &name, None)
    }

    fn is_self_type(unit: &CompileUnit<'tcx>, symbol: &Symbol) -> bool {
        unit.interner().resolve_owned(symbol.name).as_deref() == Some("Self")
    }
}

impl<'tcx> AstVisitorRust<'tcx, BinderScopes<'tcx>> for BinderVisitor<'tcx> {
    fn visit_source_file(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let file_path = unit.file_path().expect("no file path found to compile");
        let depth = scopes.scope_depth();

        // Process crate scope
        if let Some(crate_name) = parse_crate_name(file_path) {
            let symbol = if scopes.scope_depth() > 0 {
                scopes.lookup_or_insert_global(&crate_name, node, SymKind::Crate)
            } else {
                return;
            };

            if let Some(symbol) = symbol
                && let Some(scope_id) = symbol.scope()
            {
                scopes.push_scope(scope_id);
            }
        }

        if let Some(scope_id) = parse_module_name(file_path).and_then(|module_name| {
            scopes
                .lookup_or_insert(&module_name, node, SymKind::Module)
                .and_then(|symbol| symbol.scope())
        }) {
            scopes.push_scope(scope_id);
        }

        if let Some(file_name) = parse_file_name(file_path) {
            let file_sym_opt = if scopes.scope_depth() > 0 {
                scopes.lookup_or_insert(&file_name, node, SymKind::File)
            } else {
                return;
            };

            if let Some(symbol) = file_sym_opt
                && let Some(scope_id) = symbol.scope()
            {
                scopes.push_scope(scope_id);
            }
        }

        self.visit_children(unit, node, scopes, namespace, parent);
        scopes.pop_until(depth);
    }

    fn visit_mod_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        if node.child_by_field(*unit, LangRust::field_body).is_none() {
            return;
        }

        let Some(sn) = node.as_scope() else {
            self.visit_children(unit, node, scopes, _namespace, _parent);
            return;
        };
        let depth = scopes.scope_depth();
        Self::push_scope_node(scopes, sn);
        self.visit_children(unit, node, scopes, _namespace, _parent);
        scopes.pop_until(depth);
    }

    fn visit_function_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Process return type if present
        if let Some(ret_ty) = node.child_by_field(*unit, LangRust::field_return_type) {
            self.visit_node(unit, &ret_ty, scopes, namespace, parent);
        }

        // Get the scope node
        let sn = node.as_scope().unwrap();

        // Find or create symbol for the return type
        let ret_node = node.child_by_field(*unit, LangRust::field_return_type);
        let ty = ret_node
            .as_ref()
            .and_then(|ret_ty| {
                if let Some(ident) = ret_ty.find_identifier(*unit)
                    && ident.name == "Self"
                    && let Some(p) = parent
                {
                    return Some(p);
                }
                Self::resolve_type_from_node(unit, ret_ty, scopes)
            })
            .unwrap_or_else(|| {
                // Default to void/unit type if no return type found
                scopes
                    .lookup_or_insert_global("void_fn", node, SymKind::Primitive)
                    .expect("void_fn type should exist")
            });

        let func_symbol = sn.opt_ident().map(|ident| ident.symbol());
        if let Some(symbol) = func_symbol {
            if symbol.type_of().is_none() {
                symbol.set_type_of(ty.id());
            }
            symbol.add_dependency(ty);
            if let Some(ret_ty) = ret_node.as_ref() {
                Self::link_type_references(unit, ret_ty, scopes, symbol, None);
            }
        }

        let depth = scopes.scope_depth();
        let child_parent = func_symbol.or(parent);
        Self::push_scope_node(scopes, sn);
        self.visit_children(unit, node, scopes, namespace, child_parent);
        scopes.pop_until(depth);
    }

    fn visit_struct_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            let depth = scopes.scope_depth();
            let child_parent = sn.opt_ident().map(|ident| ident.symbol()).or(parent);
            Self::push_scope_node(scopes, sn);
            self.visit_children(unit, node, scopes, namespace, child_parent);
            scopes.pop_until(depth);
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    fn visit_enum_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            let depth = scopes.scope_depth();
            let child_parent = sn.opt_ident().map(|ident| ident.symbol()).or(parent);
            Self::push_scope_node(scopes, sn);
            self.visit_children(unit, node, scopes, namespace, child_parent);
            scopes.pop_until(depth);
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    fn visit_trait_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            let depth = scopes.scope_depth();
            let child_parent = sn.opt_ident().map(|ident| ident.symbol()).or(parent);
            Self::push_scope_node(scopes, sn);
            self.visit_children(unit, node, scopes, namespace, child_parent);
            scopes.pop_until(depth);
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    fn visit_impl_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let depth = scopes.scope_depth();
        let type_node = node.child_by_field(*unit, LangRust::field_type);

        if let Some(type_node_ref) = &type_node
            && let Some(target) = Self::resolve_type_from_node(unit, type_node_ref, scopes)
            && let Some(scope_id) = target.scope()
        {
            Self::link_type_references(unit, type_node_ref, scopes, target, None);
            scopes.push_scope_recursive(scope_id);
            self.visit_children(unit, node, scopes, namespace, Some(target));
            scopes.pop_until(depth);
        }
    }

    fn visit_function_signature_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(symbol) = Self::symbol_from_field(unit, node, scopes, LangRust::field_name) {
            self.set_symbol_type_from_field(
                unit,
                node,
                scopes,
                symbol,
                parent,
                LangRust::field_return_type,
            );
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_macro_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            let depth = scopes.scope_depth();
            Self::push_scope_node(scopes, sn);
            self.visit_children(unit, node, scopes, namespace, parent);
            scopes.pop_until(depth);
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    fn visit_macro_invocation(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(caller) = parent
            && let Some(target) = self.resolve_macro_symbol(unit, node, scopes)
        {
            caller.add_dependency(target);
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_const_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(symbol) = Self::symbol_from_field(unit, node, scopes, LangRust::field_name) {
            self.set_symbol_type_from_field(
                unit,
                node,
                scopes,
                symbol,
                parent,
                LangRust::field_type,
            );
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_static_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(symbol) = Self::symbol_from_field(unit, node, scopes, LangRust::field_name) {
            self.set_symbol_type_from_field(
                unit,
                node,
                scopes,
                symbol,
                parent,
                LangRust::field_type,
            );
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_call_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(caller) = parent
            && let Some(callee) = self.resolve_call_target(unit, node, scopes, Some(caller))
        {
            caller.add_dependency(callee);
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_type_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(symbol) = Self::symbol_from_field(unit, node, scopes, LangRust::field_name) {
            self.set_symbol_type_from_field(
                unit,
                node,
                scopes,
                symbol,
                parent,
                LangRust::field_type,
            );
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_type_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(symbol) = Self::symbol_from_field(unit, node, scopes, LangRust::field_name) {
            self.set_symbol_type_from_field(
                unit,
                node,
                scopes,
                symbol,
                parent,
                LangRust::field_default_type,
            );
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_associated_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(symbol) = Self::symbol_from_field(unit, node, scopes, LangRust::field_name) {
            self.set_symbol_type_from_field(
                unit,
                node,
                scopes,
                symbol,
                parent,
                LangRust::field_default_type,
            );
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_field_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(symbol) = Self::symbol_from_field(unit, node, scopes, LangRust::field_name) {
            self.set_symbol_type_from_field(
                unit,
                node,
                scopes,
                symbol,
                parent,
                LangRust::field_type,
            );
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_enum_variant(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(symbol) = Self::symbol_from_field(unit, node, scopes, LangRust::field_name) {
            let has_direct_value = node.child_by_field(*unit, LangRust::field_value).is_some();
            self.set_symbol_type_from_field(
                unit,
                node,
                scopes,
                symbol,
                parent,
                LangRust::field_value,
            );
            if !has_direct_value {
                for child_id in node.children() {
                    let child = unit.hir_node(*child_id);
                    if child.field_id() == LangRust::field_name {
                        continue;
                    }
                    Self::link_type_references(unit, &child, scopes, symbol, parent);
                }
            }
        } else if let Some(type_node) = node.child_by_field(*unit, LangRust::field_value)
            && let Some(owner_symbol) = parent
        {
            Self::link_type_references(unit, &type_node, scopes, owner_symbol, None);
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(symbol) = Self::symbol_from_field(unit, node, scopes, LangRust::field_pattern) {
            self.set_symbol_type_from_field(
                unit,
                node,
                scopes,
                symbol,
                parent,
                LangRust::field_type,
            );
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_self_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_let_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Try to get explicit type
        let mut type_symbol = None;
        if let Some(type_node) = node.child_by_field(*unit, LangRust::field_type) {
            type_symbol = Self::resolve_type_from_node(unit, &type_node, scopes);
        }

        // If no explicit type, try to infer from value
        if type_symbol.is_none()
            && let Some(value_node) = node.child_by_field(*unit, LangRust::field_value)
        {
            type_symbol = Self::infer_type_from_expr(unit, &value_node, scopes);
        }

        // Assign type to pattern
        if let Some(ty) = type_symbol {
            if let Some(pattern) = node.child_by_field(*unit, LangRust::field_pattern) {
                Self::assign_type_to_pattern(unit, &pattern, ty, scopes);

                if let Some(type_node) = node.child_by_field(*unit, LangRust::field_type) {
                    Self::link_pattern_type_references(unit, &pattern, &type_node, scopes, parent);
                }
            }

            // Also link dependency if we have a parent (e.g. function)
            if let Some(owner) = parent {
                owner.add_dependency(ty);
            }
        }

        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_block(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            // Only push scope if it was successfully created in collect phase
            if sn.scope.read().is_some() {
                let depth = scopes.scope_depth();
                Self::push_scope_node(scopes, sn);
                self.visit_children(unit, node, scopes, namespace, parent);
                scopes.pop_until(depth);
            } else {
                self.visit_children(unit, node, scopes, namespace, parent);
            }
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }
}

pub fn bind_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    namespace: &'tcx Scope<'tcx>,
    _config: &ResolverOption,
) {
    let mut visit = BinderVisitor::new();
    visit.initialize(node, scopes);
    visit.visit_node(&unit, node, scopes, namespace, None);
}

#[cfg(test)]
mod tests {
    use crate::token::LangRust;
    use llmcc_core::context::CompileCtxt;
    use llmcc_core::ir_builder::{IrBuildOption, build_llmcc_ir};
    use llmcc_core::symbol::{SymId, SymKind};
    use llmcc_resolver::{ResolverOption, bind_symbols_with, collect_symbols_with};
    use pretty_assertions::assert_eq;

    fn with_compiled_unit<F>(sources: &[&str], check: F)
    where
        F: for<'a> FnOnce(&'a CompileCtxt<'a>),
    {
        let bytes = sources
            .iter()
            .map(|src| src.as_bytes().to_vec())
            .collect::<Vec<_>>();
        let cc = CompileCtxt::from_sources::<LangRust>(&bytes);
        build_llmcc_ir::<LangRust>(&cc, IrBuildOption).unwrap();
        let resolver_option = ResolverOption::default()
            .with_sequential(true)
            .with_print_ir(true);
        let globals = collect_symbols_with::<LangRust>(&cc, &resolver_option);
        bind_symbols_with::<LangRust>(&cc, globals, &resolver_option);
        check(&cc);
    }

    fn find_symbol_id(cc: &CompileCtxt<'_>, name: &str, kind: SymKind) -> SymId {
        let name_key = cc.interner.intern(name);
        cc.symbol_map
            .read()
            .iter()
            .find(|(_, symbol)| symbol.name == name_key && symbol.kind() == kind)
            .map(|(id, _)| *id)
            .unwrap_or_else(|| panic!("symbol {name} with kind {:?} not found", kind))
    }

    fn type_name_of(cc: &CompileCtxt<'_>, sym_id: SymId) -> Option<String> {
        let map = cc.symbol_map.read();
        let symbol = map.get(&sym_id).copied()?;
        let ty_id = symbol.type_of();
        drop(map);
        let ty_id = ty_id?;
        let map = cc.symbol_map.read();
        let ty_symbol = map.get(&ty_id).copied()?;
        cc.interner.resolve_owned(ty_symbol.name)
    }

    fn assert_symbol_type(source: &[&str], name: &str, kind: SymKind, expected: Option<&str>) {
        with_compiled_unit(source, |cc| {
            let sym_id = find_symbol_id(cc, name, kind);
            let actual = type_name_of(cc, sym_id);
            assert_eq!(
                actual.as_deref(),
                expected,
                "type mismatch for symbol {name}"
            );
        });
    }

    #[test]
    fn test_shadowing_basic() {
        let source = r#"
fn run() {
    let x = 1; // i32
    {
        let x = 1.0; // f64
        let y = x; // should be f64
    }
    let z = x; // should be i32
}
"#;
        // We can't easily check "y" and "z" types directly by name because "x" is shadowed.
        // But we can check "y" and "z".
        assert_symbol_type(&[source], "y", SymKind::Variable, Some("f64"));
        assert_symbol_type(&[source], "z", SymKind::Variable, Some("i32"));
    }

    #[test]
    fn test_type_inference_literals() {
        let source = r#"
fn run() {
    let a = 42;
    let b = 3.14;
    let c = "hello";
    let d = true;
}
"#;
        assert_symbol_type(&[source], "a", SymKind::Variable, Some("i32"));
        assert_symbol_type(&[source], "b", SymKind::Variable, Some("f64"));
        assert_symbol_type(&[source], "c", SymKind::Variable, Some("str"));
        assert_symbol_type(&[source], "d", SymKind::Variable, Some("bool"));
    }

    #[test]
    fn test_type_inference_binary_ops() {
        let source = r#"
fn run() {
    let a = 1 + 2;
    let b = 1.0 * 2.0;
    let c = 1 == 2;
    let d = true && false;
}
"#;
        assert_symbol_type(&[source], "a", SymKind::Variable, Some("i32"));
        assert_symbol_type(&[source], "b", SymKind::Variable, Some("f64"));
        assert_symbol_type(&[source], "c", SymKind::Variable, Some("bool"));
        assert_symbol_type(&[source], "d", SymKind::Variable, Some("bool"));
    }

    #[test]
    fn test_type_inference_struct_field_access() {
        let source = r#"
struct Point {
    x: i32,
    y: f64,
}

fn run() {
    let p = Point { x: 1, y: 2.0 };
    let px = p.x;
    let py = p.y;
}
"#;
        assert_symbol_type(&[source], "p", SymKind::Variable, Some("Point"));
        assert_symbol_type(&[source], "px", SymKind::Variable, Some("i32"));
        assert_symbol_type(&[source], "py", SymKind::Variable, Some("f64"));
    }

    #[test]
    fn test_type_inference_function_return() {
        let source = r#"
struct User;
fn get_user() -> User { User }

fn run() {
    let u = get_user();
}
"#;
        assert_symbol_type(&[source], "u", SymKind::Variable, Some("User"));
    }

    #[test]
    fn test_type_inference_chain() {
        let source = r#"
fn run() {
    let a = 10;
    let b = a;
    let c = b;
}
"#;
        assert_symbol_type(&[source], "c", SymKind::Variable, Some("i32"));
    }
}
