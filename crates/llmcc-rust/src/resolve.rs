use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::lang_def::LanguageTrait;
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_resolver::BinderScopes;

use crate::token::LangRust;

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum BinaryOperatorOutcome {
    ReturnsBool,
    ReturnsLeftOperand,
}

pub const BINARY_OPERATOR_TOKENS: &[(u16, BinaryOperatorOutcome)] = &[
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

pub fn is_identifier_kind(kind_id: u16) -> bool {
    matches!(
        kind_id,
        LangRust::identifier
            | LangRust::scoped_identifier
            | LangRust::field_identifier
            | LangRust::type_identifier
    )
}

pub struct ExprResolver<'a, 'tcx> {
    pub unit: &'a CompileUnit<'tcx>,
    pub scopes: &'a BinderScopes<'tcx>,
}

impl<'a, 'tcx> ExprResolver<'a, 'tcx> {
    pub fn new(unit: &'a CompileUnit<'tcx>, scopes: &'a BinderScopes<'tcx>) -> Self {
        Self { unit, scopes }
    }

    pub fn normalize_identifier(name: &str) -> String {
        name.rsplit("::").next().unwrap_or(name).to_string()
    }

    pub fn identifier_name(&self, node: &HirNode<'tcx>) -> Option<String> {
        if let Some(ident) = node.as_ident() {
            return Some(Self::normalize_identifier(&ident.name));
        }
        if let Some(ident) = node.find_identifier(*self.unit) {
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

    /// Follow `type_of` chains to find the canonical definition for a symbol.
    pub fn resolve_canonical_type(
        unit: &CompileUnit<'tcx>,
        mut symbol: &'tcx Symbol,
    ) -> &'tcx Symbol {
        let mut depth = 0;
        while depth < 8 {
            let Some(target_id) = symbol.type_of() else {
                break;
            };
            let Some(next) = unit.opt_get_symbol(target_id) else {
                break;
            };
            if next.id() == symbol.id() {
                break;
            }
            symbol = next;
            depth += 1;
        }
        symbol
    }

    /// Resolve a field on a type, returning the field symbol and its optional type.
    pub fn resolve_field_type(
        &mut self,
        owner: &'tcx Symbol,
        field_name: &str,
    ) -> Option<(&'tcx Symbol, Option<&'tcx Symbol>)> {
        let owner = Self::resolve_canonical_type(self.unit, owner);
        let scope_id = owner.opt_scope()?;
        let scope = self.unit.cc.get_scope(scope_id);
        let field_key = self.unit.cc.interner.intern(field_name);
        let field_symbol = scope.lookup_symbols(field_key)?.last().copied()?;
        let field_type = field_symbol
            .type_of()
            .and_then(|ty_id| self.unit.opt_get_symbol(ty_id));
        Some((field_symbol, field_type))
    }

    pub fn first_child_node(&self, node: &HirNode<'tcx>) -> Option<HirNode<'tcx>> {
        let child_id = node.children().first()?;
        Some(self.unit.hir_node(*child_id))
    }

    pub fn lookup_callable_symbol(&self, name: &str) -> Option<&'tcx Symbol> {
        self.scopes.lookup_symbol_with(
            name,
            Some(vec![SymKind::Function, SymKind::Macro]),
            None,
            None,
        )
    }

    pub fn resolve_crate_root(&self) -> Option<&'tcx Symbol> {
        if let Some(sym) = self.scopes.lookup_symbol("crate")
            && sym.kind() == SymKind::Crate
        {
            return Some(sym);
        }
        self.scopes.scopes().iter().into_iter().find_map(|scope| {
            if let Some(sym) = scope.opt_symbol()
                && sym.kind() == SymKind::Crate
            {
                return Some(sym);
            }
            None
        })
    }

    pub fn resolve_super_relative_to(&self, anchor: Option<&Symbol>) -> Option<&'tcx Symbol> {
        let stack = self.scopes.scopes().iter();

        let base_index = if let Some(anchor_sym) = anchor {
            let anchor_scope_id = anchor_sym.scope();
            stack
                .iter()
                .rposition(|scope| scope.id() == anchor_scope_id)?
        } else {
            stack.iter().enumerate().rev().find_map(|(idx, scope)| {
                if let Some(sym) = scope.opt_symbol()
                    && matches!(
                        sym.kind(),
                        SymKind::Module | SymKind::File | SymKind::Crate | SymKind::Namespace
                    )
                {
                    return Some(idx);
                }
                None
            })?
        };

        stack.iter().take(base_index).rev().find_map(|scope| {
            if let Some(sym) = scope.opt_symbol()
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

    pub fn resolve_scoped_identifier_type(
        &mut self,
        node: &HirNode<'tcx>,
        _caller: Option<&Symbol>,
    ) -> Option<&'tcx Symbol> {
        let children = node.children_nodes(self.unit);
        let non_trivia: Vec<_> = children
            .iter()
            .filter(|child| !matches!(child.kind(), HirKind::Text | HirKind::Comment))
            .collect();

        // Handle paths starting with :: (crate root reference)
        // e.g., ::f or ::g::h - the leading :: gets filtered as Text
        if non_trivia.len() == 1 {
            let name_node = non_trivia.first()?;
            let name = self.identifier_name(name_node)?;
            // This is a crate-root reference like ::f
            // Look up in global scope
            return self.scopes.lookup_global_symbol(&name);
        }

        if non_trivia.len() < 2 {
            return None;
        }

        let path_node = non_trivia.first()?;
        let name_node = non_trivia.last()?;
        let name = self.identifier_name(name_node)?;

        let path_symbol = if path_node.kind_id() == LangRust::scoped_identifier {
            self.resolve_scoped_identifier_type(path_node, None)?
        } else if path_node.kind_id() == LangRust::super_token {
            self.resolve_super_relative_to(None)?
        } else if path_node.kind_id() == LangRust::crate_token {
            self.resolve_crate_root()?
        } else {
            let path_name = self.identifier_name(path_node)?;
            self.scopes.lookup_symbol(&path_name)?
        };

        if name_node.kind_id() == LangRust::super_token {
            return self.resolve_super_relative_to(Some(path_symbol));
        }

        self.scopes.lookup_member_symbol(path_symbol, &name, None)
    }

    pub fn infer_type_from_expr_from_node(
        &mut self,
        type_node: &HirNode<'tcx>,
    ) -> Option<&'tcx Symbol> {
        if (type_node.kind_id() == LangRust::scoped_identifier
            || type_node.kind_id() == LangRust::scoped_type_identifier)
            && let Some(sym) = self.resolve_scoped_identifier_type(type_node, None)
        {
            return Some(sym);
        }

        let ident = type_node.find_identifier(*self.unit)?;

        if let Some(existing) = self.scopes.lookup_symbol_with(
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

        self.scopes
            .lookup_or_insert_global(&ident.name, type_node, SymKind::UnresolvedType)
    }

    /// Resolve a type expression node by first trying the syntactic path
    /// (`infer_type_from_expr_from_node`) and falling back to the general
    /// expression inference when necessary.
    pub fn resolve_type_node(&mut self, type_node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        self.infer_type_from_expr_from_node(type_node)
            .or_else(|| self.infer_type_from_expr(type_node))
    }

    /// Resolve a type and gather its explicit type arguments in one pass.
    pub fn resolve_type_with_args(
        &mut self,
        node: &HirNode<'tcx>,
    ) -> (Option<&'tcx Symbol>, Vec<&'tcx Symbol>) {
        let ty = match self.resolve_type_node(node) {
            some @ Some(_) => some,
            None => self.infer_type_from_expr(node),
        };
        let args = self.collect_type_argument_symbols(node);
        (ty, args)
    }

    pub fn is_self_type(&self, symbol: &Symbol) -> bool {
        self.unit.interner().resolve_owned(symbol.name).as_deref() == Some("Self")
    }

    fn primitive_type(&mut self, node: &HirNode<'tcx>, primitive: &str) -> Option<&'tcx Symbol> {
        self.scopes
            .lookup_or_insert_global(primitive, node, SymKind::Primitive)
    }

    /// Infers primitive types for literal nodes (e.g., `42` ⇒ `i32`).
    fn infer_literal_kind(&mut self, kind_id: u16, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        if kind_id == LangRust::integer_literal {
            return self.primitive_type(node, "i32");
        }
        if kind_id == LangRust::float_literal {
            return self.primitive_type(node, "f64");
        }
        if kind_id == LangRust::string_literal {
            return self.primitive_type(node, "str");
        }
        if kind_id == LangRust::boolean_literal {
            return self.primitive_type(node, "bool");
        }
        if kind_id == LangRust::char_literal {
            return self.primitive_type(node, "char");
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
        &mut self,
        node: &HirNode<'tcx>,
        left_child: &HirNode<'tcx>,
        outcome: BinaryOperatorOutcome,
    ) -> Option<&'tcx Symbol> {
        match outcome {
            BinaryOperatorOutcome::ReturnsBool => self.primitive_type(node, "bool"),
            BinaryOperatorOutcome::ReturnsLeftOperand => self.infer_type_from_expr(left_child),
        }
    }

    /// Handles struct literal expressions (e.g., `Foo { .. }`).
    fn infer_struct_expression_type(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        node.child_by_field(*self.unit, LangRust::field_name)
            .or_else(|| node.child_by_field(*self.unit, LangRust::field_type))
            .and_then(|ty| self.infer_type_from_expr_from_node(&ty))
    }

    /// Infers types for call expressions (e.g., `Foo::new()`).
    fn infer_call_expression_type(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        node.child_by_field(*self.unit, LangRust::field_function)
            .and_then(|func| self.infer_type_from_expr(&func))
    }

    /// Infers the type of `if` expressions (based on branches).
    fn infer_if_expression_type(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        node.child_by_field(*self.unit, LangRust::field_consequence)
            .and_then(|consequence| self.infer_block_type(&consequence))
    }

    /// Handles unary operators like `*ptr` or `&value`.
    fn infer_unary_expression_type(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        node.child_by_field(*self.unit, LangRust::field_argument)
            .and_then(|operand| self.infer_type_from_expr(&operand))
    }

    /// Handles arithmetic/logical operators (e.g., `a + b`).
    fn infer_binary_operator_type(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let children = node.children_nodes(self.unit);
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
            return self.binary_operator_type(node, left_child, outcome);
        }

        // Strategy 2: Check the text between the first and second child (e.g. " + ")
        if children.len() >= 2 {
            let left = &children[0];
            let right = &children[1];
            let start = left.end_byte();
            let end = right.start_byte();

            // Ensure there is space between nodes to contain an operator
            if start < end {
                let operator = self.unit.get_text(start, end);
                if let Some(outcome) = Self::binary_operator_outcome_by_text(operator.as_str()) {
                    return self.binary_operator_type(node, left, outcome);
                }
            }
        }

        None
    }

    /// Infers the type for a field access expression like `foo.bar`.
    fn infer_field_expression_type(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        // Locate the receiver portion of the field expression (`foo` in `foo.bar`).
        let value_node = node.child_by_field(*self.unit, LangRust::field_value)?;
        // Fetch the AST node that represents the field name.
        let field_node = node.child_by_field(*self.unit, LangRust::field_field)?;
        let field_ident = field_node.as_ident()?;

        // Infer the type symbol associated with the receiver expression.
        let obj_type_symbol = self.infer_type_from_expr(&value_node)?;
        // Query the receiver's scope for the field symbol so we can read its metadata.
        let field_symbol = self
            .scopes
            .lookup_member_symbol(obj_type_symbol, &field_ident.name, Some(SymKind::Field))
            .or_else(|| {
                self.scopes.lookup_member_symbol(
                    obj_type_symbol,
                    &field_ident.name,
                    Some(SymKind::Function),
                )
            })?;
        // Extract the type ID stored on the field symbol.
        let field_type_id = field_symbol.type_of()?;
        // Resolve the type identifier into the corresponding symbol in the compile unit.
        let ty = self.unit.opt_get_symbol(field_type_id)?;

        if self.is_self_type(ty) {
            return Some(obj_type_symbol);
        }
        Some(ty)
    }

    /// Handles string/char literal nodes.
    fn infer_text_literal_type(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let text = node.as_text()?;
        let value = text.text.as_str();
        if value.chars().all(|c| c.is_ascii_digit()) {
            return self.primitive_type(node, "i32");
        }
        if value == "true" || value == "false" {
            return self.primitive_type(node, "bool");
        }
        if value.starts_with('"') {
            return self.primitive_type(node, "str");
        }
        if value.contains('.') && value.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return self.primitive_type(node, "f64");
        }
        None
    }

    /// Resolves identifiers to their known types.
    fn infer_identifier_type(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let ident = node.as_ident()?;
        let symbol = self.scopes.lookup_symbol(&ident.name)?;
        match symbol.type_of() {
            Some(type_id) => self.unit.opt_get_symbol(type_id),
            None => Some(symbol),
        }
    }

    /// Returns the first meaningful child expression (skipping trivia).
    fn infer_first_non_trivia_child(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        for child_id in node.children() {
            let child = self.unit.hir_node(*child_id);
            if !matches!(child.kind(), HirKind::Text | HirKind::Comment)
                && let Some(ty) = self.infer_type_from_expr(&child)
            {
                return Some(ty);
            }
        }
        None
    }

    /// Recurses for internal nodes that wrap actual expressions.
    fn infer_internal_node_type(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        if let Some(ty) = self.infer_binary_operator_type(node) {
            return Some(ty);
        }
        self.infer_first_non_trivia_child(node)
    }

    pub fn infer_block_type(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        // Get the last child that isn't whitespace/comments
        let last_expr = node.children().iter().rev().find(|child_id| {
            let child = self.unit.hir_node(**child_id);
            !matches!(child.kind(), HirKind::Text | HirKind::Comment)
        });

        if let Some(last_id) = last_expr {
            let last_node = self.unit.hir_node(*last_id);
            self.infer_type_from_expr(&last_node)
        } else {
            None
        }
    }

    /// Returns the type symbol for an expression node
    /// Main entry for inferring the type of an expression node.
    pub fn infer_type_from_expr(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let kind_id = node.kind_id();

        if let Some(literal_ty) = self.infer_literal_kind(kind_id, node) {
            return Some(literal_ty);
        }

        match kind_id {
            kind if kind == LangRust::scoped_identifier => {
                if let Some(sym) = self.resolve_scoped_identifier_type(node, None) {
                    if let Some(type_id) = sym.type_of()
                        && let Some(ty) = self.unit.opt_get_symbol(type_id)
                    {
                        if self.is_self_type(ty)
                            && let Some(parent_scope_id) = sym.parent_scope()
                        {
                            let parent_scope = self.unit.get_scope(parent_scope_id);
                            if let Some(parent_sym) = parent_scope.opt_symbol() {
                                return Some(parent_sym);
                            }
                        }
                        return Some(ty);
                    }
                    return Some(sym);
                }
            }
            kind if kind == LangRust::struct_expression => {
                if let Some(ty) = self.infer_struct_expression_type(node) {
                    return Some(ty);
                }
            }
            kind if kind == LangRust::call_expression => {
                if let Some(ty) = self.infer_call_expression_type(node) {
                    return Some(ty);
                }
            }
            kind if kind == LangRust::if_expression => {
                if let Some(ty) = self.infer_if_expression_type(node) {
                    return Some(ty);
                }
            }
            kind if kind == LangRust::block => {
                return self.infer_block_type(node);
            }
            kind if kind == LangRust::unary_expression => {
                if let Some(ty) = self.infer_unary_expression_type(node) {
                    return Some(ty);
                }
            }
            kind if kind == LangRust::binary_expression => {
                if let Some(ty) = self.infer_binary_operator_type(node) {
                    return Some(ty);
                }
            }
            kind if kind == LangRust::field_expression => {
                // p.x
                if let Some(ty) = self.infer_field_expression_type(node) {
                    return Some(ty);
                }
            }
            _ => {}
        }

        match node.kind() {
            HirKind::Identifier => self.infer_identifier_type(node),
            HirKind::Internal => self.infer_internal_node_type(node),
            HirKind::Text => self.infer_text_literal_type(node),
            _ => self.infer_first_non_trivia_child(node),
        }
    }

    /// Resolves an arbitrary expression down to the callable symbol it ultimately refers to.
    pub fn resolve_expression_symbol(
        &mut self,
        node: &HirNode<'tcx>,
        caller: Option<&Symbol>,
    ) -> Option<&'tcx Symbol> {
        // Inspect the syntax kind so resolution can specialize per expression form.
        match node.kind_id() {
            kind if is_identifier_kind(kind) => {
                if kind == LangRust::scoped_identifier {
                    return self.resolve_scoped_identifier_type(node, caller);
                }
                // Extract the identifier text (dropping any path prefixes).
                let name = self.identifier_name(node)?;
                // Look up the callable directly in the current scope chain.
                self.lookup_callable_symbol(&name)
            }
            kind if kind == LangRust::field_expression => {
                // Grab the AST node that names the field portion of the access.
                let field = node.child_by_field(*self.unit, LangRust::field_field)?;
                let name = self.identifier_name(&field)?;
                // Attempt to resolve `Type::field` style associated function before assuming a method.
                if let Some(symbol) = self.lookup_callable_symbol(&name) {
                    return Some(symbol);
                }

                // Resolve the receiver expression so we know which type owns the field.
                let value = node.child_by_field(*self.unit, LangRust::field_value)?;
                if let Some(obj_type) = self.infer_type_from_expr(&value) {
                    // If the type exposes a callable member with this name, treat it as a method.
                    if let Some(method) =
                        self.scopes
                            .lookup_member_symbol(obj_type, &name, Some(SymKind::Function))
                    {
                        return Some(method);
                    }
                }
                None
            }
            kind if kind == LangRust::reference_expression => {
                // Strip the reference operator and resolve the underlying expression.
                let value = node.child_by_field(*self.unit, LangRust::field_value)?;
                self.resolve_expression_symbol(&value, caller)
            }
            kind if kind == LangRust::call_expression => {
                // Look at the callable expression being invoked.
                let inner = node.child_by_field(*self.unit, LangRust::field_function)?;
                self.resolve_expression_symbol(&inner, caller)
            }
            kind if kind == LangRust::await_expression
                || kind == LangRust::try_expression
                || kind == LangRust::parenthesized_expression =>
            {
                // Dig into the wrapped expression (await, try, or parentheses).
                let child = self.first_child_node(node)?;
                self.resolve_expression_symbol(&child, caller)
            }
            _ => {
                // Fall back to treating the node as an identifier-like value.
                let name = self.identifier_name(node)?;
                self.lookup_callable_symbol(&name)
            }
        }
    }

    /// Returns the callable symbol referenced by a `call_expression`.
    pub fn resolve_call_target(
        &mut self,
        node: &HirNode<'tcx>,
        caller: Option<&Symbol>,
    ) -> Option<&'tcx Symbol> {
        let function = node.child_by_field(*self.unit, LangRust::field_function)?;
        self.resolve_expression_symbol(&function, caller)
    }

    /// Collects all type symbols referenced in a generic type (e.g., Result<Foo, Bar>).
    /// Returns all type argument symbols found within the type expression.
    pub fn collect_type_argument_symbols(&mut self, node: &HirNode<'tcx>) -> Vec<&'tcx Symbol> {
        let mut symbols = Vec::new();
        self.collect_type_argument_symbols_impl(node, &mut symbols);
        symbols
    }

    fn collect_type_argument_symbols_impl(
        &mut self,
        node: &HirNode<'tcx>,
        symbols: &mut Vec<&'tcx Symbol>,
    ) {
        // For a node like generic_type, we need to recursively find all type identifiers in type_arguments
        let kind_id = node.kind_id();

        // If this is a type identifier or similar, try to resolve it
        if kind_id == LangRust::type_identifier {
            let Some(ty) = self.infer_type_from_expr_from_node(node) else {
                return;
            };
            if symbols.iter().any(|s| s.id() == ty.id()) {
                return;
            }
            symbols.push(ty);
            return;
        }

        // Recurse into children to find all type references
        for child_id in node.children() {
            let child = self.unit.hir_node(*child_id);
            // Skip text tokens
            if !matches!(child.kind(), HirKind::Text | HirKind::Comment) {
                self.collect_type_argument_symbols_impl(&child, symbols);
            }
        }
    }
}
