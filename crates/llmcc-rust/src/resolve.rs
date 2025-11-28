//! Expression resolver for Rust language.
//!
//! This module provides type inference and symbol resolution for Rust expressions.
//! It handles:
//! - Literal type inference (integers, floats, strings, etc.)
//! - Binary and unary operator type resolution
//! - Field access and method call resolution
//! - Scoped identifier resolution (e.g., `std::io::Result`)
//! - Generic type argument collection

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::lang_def::LanguageTrait;
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_resolver::BinderScopes;

use crate::token::LangRust;

/// Resolves types and symbols for Rust expressions.
///
/// This struct provides methods to:
/// - Infer types from expression nodes
/// - Resolve scoped identifiers (e.g., `foo::bar::Baz`)
/// - Look up callable symbols (functions, macros, closures)
/// - Resolve field accesses and method calls
pub struct ExprResolver<'a, 'tcx> {
    pub unit: &'a CompileUnit<'tcx>,
    pub scopes: &'a BinderScopes<'tcx>,
}

impl<'a, 'tcx> ExprResolver<'a, 'tcx> {
    pub fn new(unit: &'a CompileUnit<'tcx>, scopes: &'a BinderScopes<'tcx>) -> Self {
        Self { unit, scopes }
    }

    // ------------------------------------------------------------------------
    // Identifier Helpers
    // ------------------------------------------------------------------------

    /// Extracts the final segment from a potentially qualified name.
    ///
    /// Example: `"std::io::Result"` → `"Result"`
    pub fn normalize_identifier(name: &str) -> String {
        name.rsplit("::").next().unwrap_or(name).to_string()
    }

    /// Extracts the identifier name from a node, handling special tokens.
    pub fn identifier_name(&self, node: &HirNode<'tcx>) -> Option<String> {
        // Direct identifier
        if let Some(ident) = node.as_ident() {
            return Some(Self::normalize_identifier(&ident.name));
        }
        // Nested identifier
        if let Some(ident) = node.find_identifier(*self.unit) {
            return Some(Self::normalize_identifier(&ident.name));
        }
        // Special tokens
        match node.kind_id() {
            k if k == LangRust::super_token => Some("super".to_string()),
            k if k == LangRust::crate_token => Some("crate".to_string()),
            _ => None,
        }
    }

    /// Returns the first non-trivia child node.
    pub fn first_child_node(&self, node: &HirNode<'tcx>) -> Option<HirNode<'tcx>> {
        let child_id = node.children().first()?;
        Some(self.unit.hir_node(*child_id))
    }

    /// Checks if a symbol represents the `Self` type.
    pub fn is_self_type(&self, symbol: &Symbol) -> bool {
        self.unit.interner().resolve_owned(symbol.name).as_deref() == Some("Self")
    }

    // ------------------------------------------------------------------------
    // Symbol Resolution
    // ------------------------------------------------------------------------

    /// Follows `type_of` chains to find the canonical definition for a symbol.
    ///
    /// This resolves type aliases and references up to 8 levels deep.
    pub fn resolve_canonical_type(
        unit: &CompileUnit<'tcx>,
        mut symbol: &'tcx Symbol,
    ) -> &'tcx Symbol {
        const MAX_DEPTH: usize = 8;
        for _ in 0..MAX_DEPTH {
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
        }
        symbol
    }

    /// Resolves a field on a type, returning the field symbol and its type.
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

    /// Looks up a callable symbol (function, macro, or closure) by name.
    pub fn lookup_callable_symbol(&self, name: &str) -> Option<&'tcx Symbol> {
        self.scopes.lookup_symbol_with(
            name,
            Some(vec![SymKind::Function, SymKind::Macro, SymKind::Closure]),
            None,
            None,
        )
    }

    // ------------------------------------------------------------------------
    // Path Resolution (crate, super, scoped identifiers)
    // ------------------------------------------------------------------------

    /// Resolves the `crate` keyword to the crate root symbol.
    pub fn resolve_crate_root(&self) -> Option<&'tcx Symbol> {
        // Try direct lookup first
        if let Some(sym) = self.scopes.lookup_symbol("crate")
            && sym.kind() == SymKind::Crate
        {
            return Some(sym);
        }
        // Fall back to searching scope stack
        self.scopes.scopes().iter().iter().find_map(|scope| {
            scope
                .opt_symbol()
                .filter(|sym| sym.kind() == SymKind::Crate)
        })
    }

    /// Resolves `super` relative to an optional anchor symbol.
    ///
    /// If `anchor` is provided, finds the parent module of that symbol.
    /// Otherwise, finds the parent of the current module context.
    pub fn resolve_super_relative_to(&self, anchor: Option<&Symbol>) -> Option<&'tcx Symbol> {
        let stack = self.scopes.scopes().iter();

        // Find the base index to start searching from
        let base_index = match anchor {
            Some(anchor_sym) => {
                let anchor_scope_id = anchor_sym.scope();
                stack
                    .iter()
                    .rposition(|scope| scope.id() == anchor_scope_id)?
            }
            None => stack.iter().enumerate().rev().find_map(|(idx, scope)| {
                scope
                    .opt_symbol()
                    .and_then(|sym| Self::is_module_like(sym.kind()).then_some(idx))
            })?,
        };

        // Find the parent module
        stack.iter().take(base_index).rev().find_map(|scope| {
            scope
                .opt_symbol()
                .filter(|sym| Self::is_module_like(sym.kind()))
        })
    }

    /// Returns `true` if the symbol kind represents a module-like container.
    fn is_module_like(kind: SymKind) -> bool {
        matches!(
            kind,
            SymKind::Module | SymKind::File | SymKind::Crate | SymKind::Namespace
        )
    }

    /// Resolves a scoped identifier like `foo::bar::Baz`.
    pub fn resolve_scoped_identifier_type(
        &mut self,
        node: &HirNode<'tcx>,
        _caller: Option<&Symbol>,
    ) -> Option<&'tcx Symbol> {
        let children = node.children_nodes(self.unit);
        let non_trivia: Vec<_> = children.iter().filter(|c| !is_trivia(c)).collect();

        // Handle `::name` (crate root reference)
        if non_trivia.len() == 1 {
            let name = self.identifier_name(non_trivia.first()?)?;
            return self.scopes.lookup_global_symbol(&name);
        }

        if non_trivia.len() < 2 {
            return None;
        }

        let path_node = non_trivia.first()?;
        let name_node = non_trivia.last()?;
        let name = self.identifier_name(name_node)?;

        // Resolve the path prefix
        let path_symbol = self.resolve_path_prefix(path_node)?;

        // Handle trailing `super`
        if name_node.kind_id() == LangRust::super_token {
            return self.resolve_super_relative_to(Some(path_symbol));
        }

        self.scopes.lookup_member_symbol(path_symbol, &name, None)
    }

    /// Resolves the prefix part of a scoped path.
    fn resolve_path_prefix(&mut self, path_node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        match path_node.kind_id() {
            k if k == LangRust::scoped_identifier => {
                self.resolve_scoped_identifier_type(path_node, None)
            }
            k if k == LangRust::super_token => self.resolve_super_relative_to(None),
            k if k == LangRust::crate_token => self.resolve_crate_root(),
            _ => {
                let path_name = self.identifier_name(path_node)?;
                self.scopes.lookup_symbol(&path_name)
            }
        }
    }

    // ------------------------------------------------------------------------
    // Type Resolution
    // ------------------------------------------------------------------------

    /// Resolves a type from an explicit type annotation node.
    ///
    /// Handles scoped identifiers and looks up type symbols.
    pub fn infer_type_from_expr_from_node(
        &mut self,
        type_node: &HirNode<'tcx>,
    ) -> Option<&'tcx Symbol> {
        // Handle scoped types like `std::io::Result`
        let kind = type_node.kind_id();
        if (kind == LangRust::scoped_identifier || kind == LangRust::scoped_type_identifier)
            && let Some(sym) = self.resolve_scoped_identifier_type(type_node, None)
        {
            return Some(sym);
        }

        let ident = type_node.find_identifier(*self.unit)?;

        if let Some(symbol) = ident.opt_symbol() {
            return Some(symbol);
        }

        // Look up existing type symbol
        const TYPE_KINDS: &[SymKind] = &[
            SymKind::Struct,
            SymKind::Enum,
            SymKind::Trait,
            SymKind::TypeAlias,
            SymKind::TypeParameter,
            SymKind::Primitive,
            SymKind::UnresolvedType,
        ];

        if let Some(existing) =
            self.scopes
                .lookup_symbol_with(&ident.name, Some(TYPE_KINDS.to_vec()), None, None)
        {
            return Some(existing);
        }

        // Insert as unresolved type
        self.scopes
            .lookup_or_insert_global(&ident.name, type_node, SymKind::UnresolvedType)
    }

    /// Resolves a type expression, trying syntactic resolution first,
    /// then falling back to expression inference.
    pub fn resolve_type_node(&mut self, type_node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        self.infer_type_from_expr_from_node(type_node)
            .or_else(|| self.infer_type_from_expr(type_node))
    }

    /// Resolves a type and collects its type arguments in one pass.
    ///
    /// Example: For `Result<Foo, Bar>`, returns the `Result` symbol and `[Foo, Bar]`.
    pub fn resolve_type_with_args(
        &mut self,
        node: &HirNode<'tcx>,
    ) -> (Option<&'tcx Symbol>, Vec<&'tcx Symbol>) {
        let ty = self
            .resolve_type_node(node)
            .or_else(|| self.infer_type_from_expr(node));
        let args = self.collect_type_argument_symbols(node);
        (ty, args)
    }

    /// Looks up or creates a primitive type symbol.
    fn primitive_type(&mut self, node: &HirNode<'tcx>, name: &str) -> Option<&'tcx Symbol> {
        self.scopes
            .lookup_or_insert_global(name, node, SymKind::Primitive)
    }

    // ------------------------------------------------------------------------
    // Literal Type Inference
    // ------------------------------------------------------------------------

    /// Infers the type of a literal node.
    ///
    /// - `42` → `i32`
    /// - `3.14` → `f64`
    /// - `"hello"` → `str`
    /// - `true` → `bool`
    /// - `'a'` → `char`
    fn infer_literal_kind(&mut self, kind_id: u16, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let primitive_name = match kind_id {
            k if k == LangRust::integer_literal => "i32",
            k if k == LangRust::float_literal => "f64",
            k if k == LangRust::string_literal => "str",
            k if k == LangRust::boolean_literal => "bool",
            k if k == LangRust::char_literal => "char",
            _ => return None,
        };
        self.primitive_type(node, primitive_name)
    }

    /// Infers the type from a text token (fallback for untyped literals).
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

    // ------------------------------------------------------------------------
    // Binary Operator Type Inference
    // ------------------------------------------------------------------------

    /// Looks up the outcome for a binary operator by kind ID or text.
    fn lookup_binary_operator(
        kind_id: Option<u16>,
        text: Option<&str>,
    ) -> Option<BinaryOperatorOutcome> {
        BINARY_OPERATOR_TOKENS
            .iter()
            .find_map(|(token_id, outcome)| {
                // Match by kind ID
                if let Some(k) = kind_id
                    && *token_id == k
                {
                    return Some(*outcome);
                }
                // Match by text
                if let Some(t) = text {
                    let trimmed = t.trim();
                    if !trimmed.is_empty()
                        && let Some(token_text) = LangRust::token_str(*token_id)
                        && token_text == trimmed
                    {
                        return Some(*outcome);
                    }
                }
                None
            })
    }

    /// Computes the result type for a binary operation.
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

    /// Infers the type of a binary expression like `a + b`.
    fn infer_binary_operator_type(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let children = node.children_nodes(self.unit);
        let left_child = children.first()?;

        // Strategy 1: Find operator by child node kind
        let outcome = children
            .iter()
            .find_map(|child| Self::lookup_binary_operator(Some(child.kind_id()), None))
            .or_else(|| {
                // Strategy 2: Parse operator from text between operands
                let right = children.get(1)?;
                (left_child.end_byte() < right.start_byte()).then(|| {
                    let text = self
                        .unit
                        .get_text(left_child.end_byte(), right.start_byte());
                    Self::lookup_binary_operator(None, Some(&text))
                })?
            })?;

        self.binary_operator_type(node, left_child, outcome)
    }

    // ------------------------------------------------------------------------
    // Expression-Specific Type Inference
    // ------------------------------------------------------------------------

    /// Infers type by extracting a child field and applying an inference function.
    fn infer_child_field_type(
        &mut self,
        node: &HirNode<'tcx>,
        field_id: u16,
        infer_fn: fn(&mut Self, &HirNode<'tcx>) -> Option<&'tcx Symbol>,
    ) -> Option<&'tcx Symbol> {
        node.child_by_field(*self.unit, field_id)
            .and_then(|child| infer_fn(self, &child))
    }

    /// Infers the type of a field access expression (e.g., `foo.bar`).
    fn infer_field_expression_type(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let value_node = node.child_by_field(*self.unit, LangRust::field_value)?;
        let field_node = node.child_by_field(*self.unit, LangRust::field_field)?;
        let field_ident = field_node.as_ident()?;

        let obj_type = self.infer_type_from_expr(&value_node)?;

        // Look up field or method
        let field_symbol = self
            .scopes
            .lookup_member_symbol(obj_type, &field_ident.name, Some(SymKind::Field))
            .or_else(|| {
                self.scopes.lookup_member_symbol(
                    obj_type,
                    &field_ident.name,
                    Some(SymKind::Function),
                )
            })?;

        let field_type_id = field_symbol.type_of()?;
        let ty = self.unit.opt_get_symbol(field_type_id)?;

        // Handle `Self` type substitution
        if self.is_self_type(ty) {
            Some(obj_type)
        } else {
            Some(ty)
        }
    }

    /// Infers the type of an identifier by looking up its symbol.
    fn infer_identifier_type(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let ident = node.as_ident()?;
        let symbol = self.scopes.lookup_symbol(&ident.name)?;

        symbol
            .type_of()
            .and_then(|id| self.unit.opt_get_symbol(id))
            .or(Some(symbol))
    }

    /// Infers the type of a block by examining its last expression.
    pub fn infer_block_type(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let last_id = node
            .children()
            .iter()
            .rev()
            .find(|id| !is_trivia(&self.unit.hir_node(**id)))?;

        self.infer_type_from_expr(&self.unit.hir_node(*last_id))
    }

    /// Finds the first non-trivia child and infers its type.
    fn infer_first_non_trivia_child(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        for child_id in node.children() {
            let child = self.unit.hir_node(*child_id);
            if !is_trivia(&child)
                && let Some(ty) = self.infer_type_from_expr(&child)
            {
                return Some(ty);
            }
        }
        None
    }

    /// Infers the type of an internal AST node.
    fn infer_internal_node_type(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        self.infer_binary_operator_type(node)
            .or_else(|| self.infer_first_non_trivia_child(node))
    }

    // ------------------------------------------------------------------------
    // Main Type Inference Entry Point
    // ------------------------------------------------------------------------

    /// Infers the type of any expression node.
    ///
    /// This is the main entry point for type inference. It handles:
    /// - Literals (integers, floats, strings, booleans, chars)
    /// - Scoped identifiers (`foo::bar`)
    /// - Struct expressions (`Foo { .. }`)
    /// - Call expressions (`foo()`)
    /// - Control flow (`if`, blocks)
    /// - Operators (unary, binary)
    /// - Field access (`foo.bar`)
    pub fn infer_type_from_expr(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let kind_id = node.kind_id();

        // Try literal inference first
        if let Some(ty) = self.infer_literal_kind(kind_id, node) {
            return Some(ty);
        }

        // Handle specific expression kinds
        if let Some(ty) = self.infer_by_syntax_kind(kind_id, node) {
            return Some(ty);
        }

        // Fall back to HIR kind dispatch
        match node.kind() {
            HirKind::Identifier => self.infer_identifier_type(node),
            HirKind::Internal => self.infer_internal_node_type(node),
            HirKind::Text => self.infer_text_literal_type(node),
            _ => self.infer_first_non_trivia_child(node),
        }
    }

    /// Dispatches type inference based on syntax kind.
    fn infer_by_syntax_kind(&mut self, kind_id: u16, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        match kind_id {
            k if k == LangRust::scoped_identifier => self.infer_scoped_identifier_type(node),
            k if k == LangRust::struct_expression => {
                // Try field_name first, then field_type for struct expressions
                node.child_by_field(*self.unit, LangRust::field_name)
                    .or_else(|| node.child_by_field(*self.unit, LangRust::field_type))
                    .and_then(|ty| self.infer_type_from_expr_from_node(&ty))
            }
            k if k == LangRust::call_expression => self.infer_child_field_type(
                node,
                LangRust::field_function,
                Self::infer_type_from_expr,
            ),
            k if k == LangRust::if_expression => self.infer_child_field_type(
                node,
                LangRust::field_consequence,
                Self::infer_block_type,
            ),
            k if k == LangRust::block => self.infer_block_type(node),
            k if k == LangRust::unary_expression => self.infer_child_field_type(
                node,
                LangRust::field_argument,
                Self::infer_type_from_expr,
            ),
            k if k == LangRust::binary_expression => self.infer_binary_operator_type(node),
            k if k == LangRust::field_expression => self.infer_field_expression_type(node),
            _ => None,
        }
    }

    /// Infers the type of a scoped identifier with `Self` handling.
    fn infer_scoped_identifier_type(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let sym = self.resolve_scoped_identifier_type(node, None)?;

        // Check if symbol has a type annotation
        let type_id = sym.type_of()?;
        let ty = self.unit.opt_get_symbol(type_id)?;

        // Handle `Self` type substitution
        if self.is_self_type(ty)
            && let Some(parent_scope_id) = sym.parent_scope()
        {
            let parent_scope = self.unit.get_scope(parent_scope_id);
            if let Some(parent_sym) = parent_scope.opt_symbol() {
                return Some(parent_sym);
            }
        }

        Some(ty)
    }

    // ------------------------------------------------------------------------
    // Symbol Resolution for Callable Expressions
    // ------------------------------------------------------------------------

    /// Resolves an expression to its underlying callable symbol.
    ///
    /// This handles various expression forms that can represent callables:
    /// - Direct identifiers (`foo`)
    /// - Scoped identifiers (`foo::bar`)
    /// - Field expressions (`obj.method`)
    /// - References (`&foo`)
    /// - Call expressions (`foo()`)
    /// - Wrapped expressions (`foo.await`, `foo?`, `(foo)`)
    pub fn resolve_expression_symbol(
        &mut self,
        node: &HirNode<'tcx>,
        caller: Option<&Symbol>,
    ) -> Option<&'tcx Symbol> {
        match node.kind_id() {
            k if is_identifier_kind(k) => self.resolve_identifier_symbol(node, caller),
            k if k == LangRust::field_expression => self.resolve_field_symbol(node),
            k if k == LangRust::reference_expression => {
                self.resolve_child_field(node, LangRust::field_value, caller)
            }
            k if k == LangRust::call_expression => {
                self.resolve_child_field(node, LangRust::field_function, caller)
            }
            k if Self::is_wrapper_expression(k) => self.resolve_wrapped_symbol(node, caller),
            _ => {
                let name = self.identifier_name(node)?;
                self.lookup_callable_symbol(&name)
            }
        }
    }

    /// Checks if a syntax kind represents a wrapper expression.
    fn is_wrapper_expression(kind: u16) -> bool {
        kind == LangRust::await_expression
            || kind == LangRust::try_expression
            || kind == LangRust::parenthesized_expression
    }

    /// Resolves an identifier or scoped identifier to a symbol.
    fn resolve_identifier_symbol(
        &mut self,
        node: &HirNode<'tcx>,
        caller: Option<&Symbol>,
    ) -> Option<&'tcx Symbol> {
        if node.kind_id() == LangRust::scoped_identifier {
            return self.resolve_scoped_identifier_type(node, caller);
        }
        let name = self.identifier_name(node)?;
        self.lookup_callable_symbol(&name)
    }

    /// Resolves a field expression to its callable symbol.
    fn resolve_field_symbol(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let field = node.child_by_field(*self.unit, LangRust::field_field)?;
        let name = self.identifier_name(&field)?;

        // Try associated function first
        if let Some(symbol) = self.lookup_callable_symbol(&name) {
            return Some(symbol);
        }

        // Try method on receiver type
        let value = node.child_by_field(*self.unit, LangRust::field_value)?;
        let obj_type = self.infer_type_from_expr(&value)?;
        self.scopes
            .lookup_member_symbol(obj_type, &name, Some(SymKind::Function))
    }

    /// Resolves through a child field to find the underlying symbol.
    fn resolve_child_field(
        &mut self,
        node: &HirNode<'tcx>,
        field_id: u16,
        caller: Option<&Symbol>,
    ) -> Option<&'tcx Symbol> {
        let child = node.child_by_field(*self.unit, field_id)?;
        self.resolve_expression_symbol(&child, caller)
    }

    /// Resolves through a wrapper expression (await, try, parentheses).
    fn resolve_wrapped_symbol(
        &mut self,
        node: &HirNode<'tcx>,
        caller: Option<&Symbol>,
    ) -> Option<&'tcx Symbol> {
        let child = self.first_child_node(node)?;
        self.resolve_expression_symbol(&child, caller)
    }

    /// Returns the callable symbol referenced by a `call_expression`.
    pub fn resolve_call_target(
        &mut self,
        node: &HirNode<'tcx>,
        caller: Option<&Symbol>,
    ) -> Option<&'tcx Symbol> {
        self.resolve_child_field(node, LangRust::field_function, caller)
    }

    // ------------------------------------------------------------------------

    /// For a scoped call like `Type::method()`, returns the Type symbol.
    /// The node should be a scoped_identifier like `Type::method`.
    pub fn resolve_scoped_call_receiver(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        if node.kind_id() != LangRust::scoped_identifier {
            return None;
        }

        let children = node.children_nodes(self.unit);
        let non_trivia: Vec<_> = children.iter().filter(|c| !is_trivia(c)).collect();

        if non_trivia.len() < 2 {
            return None;
        }

        // The first part is the path (Type in Type::method)
        let path_node = non_trivia.first()?;
        self.resolve_path_prefix(path_node)
    }
    // Generic Type Argument Collection
    // ------------------------------------------------------------------------

    /// Collects all type symbols from a generic type expression.
    ///
    /// Example: For `Result<Foo, Bar>`, returns `[Foo, Bar]`.
    pub fn collect_type_argument_symbols(&mut self, node: &HirNode<'tcx>) -> Vec<&'tcx Symbol> {
        let mut symbols = Vec::new();
        self.collect_type_args_recursive(node, &mut symbols);
        symbols
    }

    /// Recursively collects type argument symbols.
    fn collect_type_args_recursive(
        &mut self,
        node: &HirNode<'tcx>,
        symbols: &mut Vec<&'tcx Symbol>,
    ) {
        // Resolve type identifiers
        if node.kind_id() == LangRust::type_identifier {
            if let Some(ty) = self.infer_type_from_expr_from_node(node)
                && !symbols.iter().any(|s| s.id() == ty.id())
            {
                symbols.push(ty);
            }
            return;
        }

        // Recurse into children
        for child_id in node.children() {
            let child = self.unit.hir_node(*child_id);
            if !is_trivia(&child) {
                self.collect_type_args_recursive(&child, symbols);
            }
        }
    }
}

/// Describes how a binary operator affects the result type.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum BinaryOperatorOutcome {
    /// Comparison/logical operators always return `bool`.
    ReturnsBool,
    /// Arithmetic operators return the same type as the left operand.
    ReturnsLeftOperand,
}

/// Mapping from token kinds to their type behavior.
///
/// - Comparison operators (`==`, `!=`, `<`, `>`, `<=`, `>=`) → `bool`
/// - Logical operators (`&&`, `||`) → `bool`
/// - Arithmetic operators (`+`, `-`, `*`, `/`, `%`) → left operand type
pub const BINARY_OPERATOR_TOKENS: &[(u16, BinaryOperatorOutcome)] = &[
    // Comparison operators → bool
    (LangRust::Text_EQEQ, BinaryOperatorOutcome::ReturnsBool), // ==
    (LangRust::Text_NE, BinaryOperatorOutcome::ReturnsBool),   // !=
    (LangRust::Text_LT, BinaryOperatorOutcome::ReturnsBool),   // <
    (LangRust::Text_GT, BinaryOperatorOutcome::ReturnsBool),   // >
    (LangRust::Text_LE, BinaryOperatorOutcome::ReturnsBool),   // <=
    (LangRust::Text_GE, BinaryOperatorOutcome::ReturnsBool),   // >=
    // Logical operators → bool
    (LangRust::Text_AMPAMP, BinaryOperatorOutcome::ReturnsBool), // &&
    (LangRust::Text_PIPEPIPE, BinaryOperatorOutcome::ReturnsBool), // ||
    // Arithmetic operators → left operand type
    (
        LangRust::Text_PLUS,
        BinaryOperatorOutcome::ReturnsLeftOperand,
    ), // +
    (
        LangRust::Text_MINUS,
        BinaryOperatorOutcome::ReturnsLeftOperand,
    ), // -
    (
        LangRust::Text_STAR,
        BinaryOperatorOutcome::ReturnsLeftOperand,
    ), // *
    (
        LangRust::Text_SLASH,
        BinaryOperatorOutcome::ReturnsLeftOperand,
    ), // /
    (
        LangRust::Text_PERCENT,
        BinaryOperatorOutcome::ReturnsLeftOperand,
    ), // %
];

/// Returns `true` if the given syntax kind represents an identifier.
fn is_identifier_kind(kind_id: u16) -> bool {
    matches!(
        kind_id,
        LangRust::identifier
            | LangRust::scoped_identifier
            | LangRust::field_identifier
            | LangRust::type_identifier
    )
}

/// Returns `true` if the HIR node is trivia (whitespace/comments).
fn is_trivia(node: &HirNode) -> bool {
    matches!(node.kind(), HirKind::Text | HirKind::Comment)
}
