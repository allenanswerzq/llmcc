use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::lang_def::LanguageTrait;
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_resolver::BinderScopes;

use crate::token::LangRust;

/// Simple type system for Rust language
///
/// infer_* - Determines the type of something:
/// resolve_* - Finds what symbol/name something refers to:
pub struct TyCtxt<'a, 'tcx> {
    pub unit: &'a CompileUnit<'tcx>,
    pub scopes: &'a BinderScopes<'tcx>,
}

impl<'a, 'tcx> TyCtxt<'a, 'tcx> {
    pub fn new(unit: &'a CompileUnit<'tcx>, scopes: &'a BinderScopes<'tcx>) -> Self {
        Self { unit, scopes }
    }

    /// Infers the type of any expression node.
    pub fn infer_expr_type(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        TyImpl::new(self).infer_expr_type(node)
    }

    /// Infers the type from an explicit type annotation node.
    pub fn infer_annotated_type(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        TyImpl::new(self).infer_annotated_type(node)
    }

    /// Resolves an expression to its underlying callable symbol.
    pub fn resolve_callable(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        TyImpl::new(self).resolve_callable(node)
    }

    /// Resolves a type node, falling back to expression inference if needed.
    pub fn resolve_type(&mut self, type_node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        self.infer_annotated_type(type_node)
            .or_else(|| self.infer_expr_type(type_node))
    }

    /// Resolves a type and collects its type arguments.
    pub fn resolve_type_and_args(
        &mut self,
        node: &HirNode<'tcx>,
    ) -> (Option<&'tcx Symbol>, Vec<&'tcx Symbol>) {
        let ty = self.resolve_type(node);
        let args = self.collect_type_args(node);
        (ty, args)
    }

    /// Collects all type symbols from a generic type expression.
    pub fn collect_type_args(&mut self, node: &HirNode<'tcx>) -> Vec<&'tcx Symbol> {
        let mut symbols = Vec::new();
        TyImpl::new(self).collect_generic_symbols(node, &mut symbols);
        symbols
    }

    /// Resolves a scoped identifier like `foo::bar::Baz`.
    pub fn resolve_scoped_path(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        TyImpl::new(self).resolve_scoped_path(node)
    }

    /// Helper: Resolves canonical type (follows aliases).
    pub fn resolve_type_of(unit: &CompileUnit<'tcx>, symbol: &'tcx Symbol) -> &'tcx Symbol {
        TyImpl::resolve_type_of(unit, symbol)
    }

    /// Helper: Resolves the receiver of a scoped call.
    pub fn resolve_path_prefix(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        TyImpl::new(self).resolve_scoped_path(node)
    }

    /// Helper: Resolves the target of a call expression.
    pub fn resolve_callee(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        TyImpl::new(self).resolve_callable_child_field(node, LangRust::field_function)
    }

    /// Helper: Check if symbol is the `Self` type.
    pub fn is_self(&self, symbol: &Symbol) -> bool {
        // Check by comparing the symbol name - interned strings can be compared
        // This is a temporary implementation; ideally we'd have better type info
        false
    }
}

// ============================================================================
// Internal Implementation
// ============================================================================

struct TyImpl<'a, 'b, 'tcx> {
    ty: &'b mut TyCtxt<'a, 'tcx>,
}

impl<'a, 'b, 'tcx> TyImpl<'a, 'b, 'tcx> {
    fn new(ty: &'b mut TyCtxt<'a, 'tcx>) -> Self {
        Self { ty }
    }

    // ========================================================================
    // Type Inference
    // ========================================================================
    fn infer_expr_type(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        match node.kind_id() {
            // Literals
            LangRust::integer_literal => self.primitive_type("i32"),
            LangRust::float_literal => self.primitive_type("f64"),
            LangRust::string_literal => self.primitive_type("str"),
            LangRust::boolean_literal => self.primitive_type("bool"),
            LangRust::char_literal => self.primitive_type("char"),

            // Structural Inference
            LangRust::scoped_identifier => self.infer_scoped_identifier(node),
            LangRust::struct_expression => self.infer_struct_expression(node),
            LangRust::call_expression => self.infer_child_field(node, LangRust::field_function),
            LangRust::if_expression => self.infer_if_expression(node),
            LangRust::block => self.infer_block(node),
            LangRust::unary_expression => self.infer_child_field(node, LangRust::field_argument),
            LangRust::binary_expression => self.infer_binary_expression(node),
            LangRust::field_expression => self.infer_field_expression(node),

            _ => None,
        }
    }

    fn infer_annotated_type(&mut self, type_node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let kind = type_node.kind_id();

        if kind == LangRust::scoped_identifier || kind == LangRust::scoped_type_identifier {
            if let Some(sym) = self.resolve_scoped_path(type_node) {
                return Some(sym);
            }
        }

        let ident = type_node.find_identifier(&self.ty.unit)?;
        if let Some(symbol) = ident.opt_symbol() {
            return Some(symbol);
        }

        const TYPE_KINDS: &[SymKind] = &[
            SymKind::Struct,
            SymKind::Enum,
            SymKind::Trait,
            SymKind::TypeAlias,
            SymKind::TypeParameter,
            SymKind::Primitive,
            SymKind::UnresolvedType,
        ];

        if let Some(existing) = self
            .ty
            .scopes
            .lookup_symbol(&ident.name, TYPE_KINDS.to_vec())
        {
            return Some(existing);
        }

        None
    }

    fn primitive_type(&mut self, name: &str) -> Option<&'tcx Symbol> {
        let symbols = self
            .ty
            .scopes
            .lookup_globals(name, vec![SymKind::Primitive])?;
        if symbols.len() > 1 {
            tracing::warn!(
                "multiple primitive types found for '{}', returning the last one",
                name
            );
        }
        symbols.last().copied()
    }

    fn infer_text_literal(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let value = self.as_text_literal(node)?;

        if value.chars().all(|c| c.is_ascii_digit()) {
            return self.primitive_type("i32");
        }
        if value == "true" || value == "false" {
            return self.primitive_type("bool");
        }
        if value.starts_with('"') {
            return self.primitive_type("str");
        }
        if value.contains('.') && value.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return self.primitive_type("f64");
        }
        None
    }

    fn infer_scoped_identifier(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let sym = self.resolve_scoped_path(node)?;
        let type_id = sym.type_of()?;
        let ty = self.ty.unit.opt_get_symbol(type_id)?;
        Some(ty)
    }

    fn infer_struct_expression(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        node.child_by_field(*self.ty.unit, LangRust::field_name)
            .or_else(|| node.child_by_field(*self.ty.unit, LangRust::field_type))
            .and_then(|ty_node| self.infer_annotated_type(&ty_node))
    }

    fn infer_if_expression(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        if let Some(consequence) = node.child_by_field(*self.ty.unit, LangRust::field_consequence) {
            return self.infer_block(&consequence);
        }
        None
    }

    fn infer_block(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let children = node.children(&self.ty.unit);
        let last_child = children
            .iter()
            .rev()
            .find(|child| !Self::is_trivia(child))?;

        self.infer_expr_type(last_child)
    }

    fn infer_binary_expression(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let (left_node, _, outcome) = self.get_binary_components(node)?;

        match outcome {
            Some(BinaryOperatorOutcome::ReturnsBool) => self.primitive_type("bool"),
            Some(BinaryOperatorOutcome::ReturnsLeftOperand) => self.infer_expr_type(&left_node),
            None => None,
        }
    }

    fn infer_field_expression(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let value_node = node.child_by_field(*self.ty.unit, LangRust::field_value)?;
        let field_node = node.child_by_field(*self.ty.unit, LangRust::field_field)?;
        let field_ident = field_node.as_ident()?;

        let obj_type = self.infer_expr_type(&value_node)?;

        let field_symbol = self
            .ty
            .scopes
            .lookup_member_symbol(obj_type, &field_ident.name, Some(SymKind::Field))
            .or_else(|| {
                self.ty.scopes.lookup_member_symbol(
                    obj_type,
                    &field_ident.name,
                    Some(SymKind::Function),
                )
            })?;

        let field_type_id = field_symbol.type_of()?;
        let field_type = self.ty.unit.opt_get_symbol(field_type_id)?;

        if self.ty.is_self(field_type) {
            Some(obj_type)
        } else {
            Some(field_type)
        }
    }

    fn infer_child_field(&mut self, node: &HirNode<'tcx>, field_id: u16) -> Option<&'tcx Symbol> {
        let child = node.child_by_field(*self.ty.unit, field_id)?;
        self.infer_expr_type(&child)
    }

    // ========================================================================
    // Symbol Resolution
    // ========================================================================

    fn resolve_callable(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        match node.kind_id() {
            // Direct identifier (name lookup)
            k if self.is_identifier_kind(k) => self.resolve_callable_identifier(node),

            // Field or method access: obj.method
            LangRust::field_expression => self.resolve_callable_field_access(node),

            // Call expression: fn()
            LangRust::call_expression => {
                self.resolve_callable_child_field(node, LangRust::field_function)
            }

            // Reference expression: &fn
            LangRust::reference_expression => {
                self.resolve_callable_child_field(node, LangRust::field_value)
            }

            // Wrapper expressions: await fn, try fn, (fn)
            k if self.is_wrapper_expression(k) => self.resolve_callable_wrapped(node),

            _ => None,
        }
    }

    fn resolve_callable_identifier(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        if node.kind_id() == LangRust::scoped_identifier {
            return self.resolve_scoped_path(node);
        } else if let Some(ident) = node.as_ident() {
            return self
                .ty
                .scopes
                .lookup_symbol(&ident.name, vec![SymKind::Function, SymKind::Closure]);
        }
        None
    }

    fn resolve_callable_field_access(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let value_node = node.child_by_field(*self.ty.unit, LangRust::field_value)?;
        let obj_type = self.infer_expr_type(&value_node)?;

        let field_node = node.child_by_field(*self.ty.unit, LangRust::field_field)?;
        if let Some(ident) = field_node.as_ident() {
            return self.ty.scopes.lookup_member_symbol(
                obj_type,
                &ident.name,
                Some(SymKind::Function),
            );
        }
        None
    }

    fn resolve_callable_child_field(
        &mut self,
        node: &HirNode<'tcx>,
        field_id: u16,
    ) -> Option<&'tcx Symbol> {
        let child_node = node.child_by_field(*self.ty.unit, field_id)?;
        self.resolve_callable(&child_node)
    }

    fn resolve_callable_wrapped(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let child_node = self.first_significant_child(node)?;
        self.resolve_callable(&child_node)
    }

    fn is_identifier_kind(&self, kind_id: u16) -> bool {
        matches!(
            kind_id,
            LangRust::identifier
                | LangRust::scoped_identifier
                | LangRust::field_identifier
                | LangRust::type_identifier
        )
    }

    fn is_wrapper_expression(&self, kind: u16) -> bool {
        matches!(
            kind,
            LangRust::await_expression
                | LangRust::try_expression
                | LangRust::parenthesized_expression
        )
    }

    // ========================================================================
    // Path Resolution
    // ========================================================================

    fn resolve_scoped_path(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let ident = node.as_ident()?;
        self.ty.scopes.lookup_symbol(&ident.name, vec![])
    }

    // ========================================================================
    // Type Canonicalization
    // ========================================================================

    fn resolve_type_of(unit: &CompileUnit<'tcx>, mut current_symbol: &'tcx Symbol) -> &'tcx Symbol {
        const MAX_DEPTH: usize = 8;
        for _ in 0..MAX_DEPTH {
            let Some(target_type_id) = current_symbol.type_of() else {
                break;
            };
            let Some(next_symbol) = unit.opt_get_symbol(target_type_id) else {
                break;
            };
            if next_symbol.id() == current_symbol.id() {
                break;
            }
            current_symbol = next_symbol;
        }
        current_symbol
    }

    fn collect_generic_symbols(
        &mut self,
        node: &HirNode<'tcx>,
        collected_symbols: &mut Vec<&'tcx Symbol>,
    ) {
        if node.kind_id() == LangRust::type_identifier {
            if let Some(type_symbol) = self.infer_annotated_type(node)
                && !collected_symbols.iter().any(|s| s.id() == type_symbol.id())
            {
                collected_symbols.push(type_symbol);
            }
            return;
        }

        let children = node.children(self.ty.unit);
        for child_node in children {
            if !Self::is_trivia(&child_node) {
                self.collect_generic_symbols(&child_node, collected_symbols);
            }
        }
    }

    fn is_trivia(node: &HirNode) -> bool {
        matches!(node.kind(), HirKind::Text | HirKind::Comment)
    }

    fn first_significant_child(&self, node: &HirNode<'tcx>) -> Option<HirNode<'tcx>> {
        node.children(self.ty.unit)
            .iter()
            .find(|child| !Self::is_trivia(child))
            .copied()
    }

    fn as_text_literal(&self, node: &HirNode<'tcx>) -> Option<&'tcx str> {
        node.as_text().map(|t| t.text.as_str())
    }

    fn get_binary_components(
        &self,
        node: &HirNode<'tcx>,
    ) -> Option<(HirNode<'tcx>, HirNode<'tcx>, Option<BinaryOperatorOutcome>)> {
        let children = node.children(self.ty.unit);
        let left = children.first().map(|child| *child)?;

        let outcome = children
            .iter()
            .find_map(|child| self.lookup_binary_operator(Some(child.kind_id()), None))
            .or_else(|| {
                let right = children.get(1).map(|child| *child)?;
                if left.end_byte() < right.start_byte() {
                    let text = self.ty.unit.get_text(left.end_byte(), right.start_byte());
                    self.lookup_binary_operator(None, Some(&text))
                } else {
                    None
                }
            });

        Some((left, left, outcome))
    }

    fn lookup_binary_operator(
        &self,
        kind_id: Option<u16>,
        text: Option<&str>,
    ) -> Option<BinaryOperatorOutcome> {
        BINARY_OPERATOR_TOKENS
            .iter()
            .find_map(|(token_id, outcome)| {
                if let Some(k) = kind_id
                    && *token_id == k
                {
                    return Some(*outcome);
                }
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
}

// ============================================================================
// Constants & Enums
// ============================================================================

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum BinaryOperatorOutcome {
    ReturnsBool,
    ReturnsLeftOperand,
}

pub const BINARY_OPERATOR_TOKENS: &[(u16, BinaryOperatorOutcome)] = &[
    (LangRust::Text_EQEQ, BinaryOperatorOutcome::ReturnsBool),
    (LangRust::Text_NE, BinaryOperatorOutcome::ReturnsBool),
    (LangRust::Text_LT, BinaryOperatorOutcome::ReturnsBool),
    (LangRust::Text_GT, BinaryOperatorOutcome::ReturnsBool),
    (LangRust::Text_LE, BinaryOperatorOutcome::ReturnsBool),
    (LangRust::Text_GE, BinaryOperatorOutcome::ReturnsBool),
    (LangRust::Text_AMPAMP, BinaryOperatorOutcome::ReturnsBool),
    (LangRust::Text_PIPEPIPE, BinaryOperatorOutcome::ReturnsBool),
    (
        LangRust::Text_PLUS,
        BinaryOperatorOutcome::ReturnsLeftOperand,
    ),
    (
        LangRust::Text_MINUS,
        BinaryOperatorOutcome::ReturnsLeftOperand,
    ),
    (
        LangRust::Text_STAR,
        BinaryOperatorOutcome::ReturnsLeftOperand,
    ),
    (
        LangRust::Text_SLASH,
        BinaryOperatorOutcome::ReturnsLeftOperand,
    ),
    (
        LangRust::Text_PERCENT,
        BinaryOperatorOutcome::ReturnsLeftOperand,
    ),
];
