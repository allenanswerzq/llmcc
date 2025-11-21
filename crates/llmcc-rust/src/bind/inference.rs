use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::lang_def::LanguageTrait;
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_resolver::BinderScopes;

use super::constants::{BINARY_OPERATOR_TOKENS, BinaryOperatorOutcome, is_identifier_kind};
use super::resolution::SymbolResolver;
use crate::token::LangRust;

pub struct TypeInferrer<'a, 'tcx> {
    pub unit: &'a CompileUnit<'tcx>,
    pub scopes: &'a mut BinderScopes<'tcx>,
}

impl<'a, 'tcx> TypeInferrer<'a, 'tcx> {
    pub fn new(unit: &'a CompileUnit<'tcx>, scopes: &'a mut BinderScopes<'tcx>) -> Self {
        Self { unit, scopes }
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
        let mut resolver = SymbolResolver::new(self.unit, self.scopes);
        node.child_by_field(*self.unit, LangRust::field_name)
            .or_else(|| node.child_by_field(*self.unit, LangRust::field_type))
            .and_then(|ty| resolver.resolve_type_from_node(&ty))
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

        let resolver = SymbolResolver::new(self.unit, self.scopes);
        if resolver.is_self_type(ty) {
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
                let mut resolver = SymbolResolver::new(self.unit, self.scopes);
                if let Some(sym) = resolver.resolve_scoped_identifier_symbol(node, None) {
                    if let Some(type_id) = sym.type_of()
                        && let Some(ty) = self.unit.opt_get_symbol(type_id)
                    {
                        if resolver.is_self_type(ty)
                            && let Some(parent_scope_id) = sym.parent_scope()
                        {
                            let parent_scope = self.unit.get_scope(parent_scope_id);
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
                let mut resolver = SymbolResolver::new(self.unit, self.scopes);
                if kind == LangRust::scoped_identifier {
                    return resolver.resolve_scoped_identifier_symbol(node, caller);
                }
                // Extract the identifier text (dropping any path prefixes).
                let name = resolver.identifier_name(node)?;
                // Look up the callable directly in the current scope chain.
                resolver.lookup_callable_symbol(&name)
            }
            kind if kind == LangRust::field_expression => {
                let resolver = SymbolResolver::new(self.unit, self.scopes);
                // Grab the AST node that names the field portion of the access.
                let field = node.child_by_field(*self.unit, LangRust::field_field)?;
                let name = resolver.identifier_name(&field)?;
                // Attempt to resolve `Type::field` style associated function before assuming a method.
                if let Some(symbol) = resolver.lookup_callable_symbol(&name) {
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
                let resolver = SymbolResolver::new(self.unit, self.scopes);
                // Dig into the wrapped expression (await, try, or parentheses).
                let child = resolver.first_child_node(node)?;
                self.resolve_expression_symbol(&child, caller)
            }
            _ => {
                let resolver = SymbolResolver::new(self.unit, self.scopes);
                // Fall back to treating the node as an identifier-like value.
                let name = resolver.identifier_name(node)?;
                resolver.lookup_callable_symbol(&name)
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
}
