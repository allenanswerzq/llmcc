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

    /// Infers the type with optional symbol kind filtering.
    /// If `kind_filter` is provided, restricts lookup to those kinds (e.g., for callable resolution).
    fn infer_expr_filtered(
        &mut self,
        node: &HirNode<'tcx>,
        kinds_filter: Option<Vec<SymKind>>,
    ) -> Option<&'tcx Symbol> {
        TyImpl::new(self).infer_expr_impl(node, kinds_filter)
    }

    /// Infers the type of any expression node without filtering.
    pub fn infer_expr(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        self.infer_expr_filtered(node, None)
    }

    /// Resolves an expression to its underlying callable symbol.
    pub fn resolve_callable(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        self.infer_expr_filtered(node, Some(vec![SymKind::Function, SymKind::Closure]))
    }

    /// Resolves a type node
    pub fn resolve_type(&mut self, type_node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        self.infer_expr_filtered(
            type_node,
            Some(vec![
                SymKind::Struct,
                SymKind::Enum,
                SymKind::Trait,
                SymKind::TypeAlias,
                SymKind::TypeParameter,
                SymKind::Primitive,
                SymKind::Macro,
                SymKind::UnresolvedType,
            ]),
        )
    }

    /// Helper: Check if symbol is the `Self` type.
    pub fn is_self(&self, symbol: &Symbol) -> bool {
        // Check by comparing the symbol name - interned strings can be compared
        // This is a temporary implementation; ideally we'd have better type info
        false
    }

    /// Collects all type symbols from a generic type expression.
    pub fn collect_types(&mut self, node: &HirNode<'tcx>) -> Vec<&'tcx Symbol> {
        let mut symbols = Vec::new();
        TyImpl::new(self).collect_types(node, &mut symbols);
        symbols
    }

    /// Resolves canonical type (follows aliases).
    pub fn resolve_type_of(unit: &CompileUnit<'tcx>, symbol: &'tcx Symbol) -> &'tcx Symbol {
        TyImpl::resolve_type_of(unit, symbol)
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

    /// calling with no kind filters
    fn infer_expr_no_filter(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        self.infer_expr_impl(node, None)
    }

    fn infer_expr_impl(
        &mut self,
        node: &HirNode<'tcx>,
        kinds_filter: Option<Vec<SymKind>>,
    ) -> Option<&'tcx Symbol> {
        match node.kind_id() {
            // Literals
            LangRust::integer_literal => self.primitive_type("i32"),
            LangRust::float_literal => self.primitive_type("f64"),
            LangRust::string_literal => self.primitive_type("str"),
            LangRust::boolean_literal => self.primitive_type("bool"),
            LangRust::char_literal => self.primitive_type("char"),

            // Structural Inference
            LangRust::scoped_identifier => self.infer_scoped_identifier(node, kinds_filter.clone()),
            LangRust::struct_expression => self.infer_struct_expression(node),
            LangRust::call_expression => self.infer_child_field(node, LangRust::field_function),
            LangRust::if_expression => self.infer_if_expression(node),
            LangRust::block => self.infer_block(node),
            LangRust::unary_expression => self.infer_child_field(node, LangRust::field_argument),
            LangRust::binary_expression => self.infer_binary_expression(node),
            LangRust::field_expression => self.infer_field_expression(node, kinds_filter.clone()),
            LangRust::primitive_type => {
                let prim_name = self.ty.unit.hir_text(node);
                self.primitive_type(&prim_name)
            }
            _ => {
                // Try to resolve as identifier first
                let ident = node.find_ident(&self.ty.unit)?;
                if let Some(symbol) = ident.opt_symbol() {
                    return Some(symbol);
                }
                // Fall back to lookup by name with kind filters
                if let Some(lookup_kinds) = kinds_filter {
                    return self.ty.scopes.lookup_symbol(&ident.name, lookup_kinds);
                }
                None
            }
        }
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

    /// Collects all child nodes that are identifiers.
    pub fn collect_idents(&self, node: &HirNode<'tcx>) -> Vec<HirNode<'tcx>> {
        let mut identifiers: Vec<HirNode<'_>> = Vec::new();
        Self::collect_ident_recursive(self.ty.unit, node, &mut identifiers);
        identifiers
    }

    fn infer_scoped_identifier(
        &mut self,
        node: &HirNode<'tcx>,
        kinds_filter: Option<Vec<SymKind>>,
    ) -> Option<&'tcx Symbol> {
        // Collect all identifier parts of the scoped path (e.g., foo::Bar::baz)
        let idents = self.collect_idents(node);

        if idents.is_empty() {
            return None;
        }

        // Extract names from identifiers
        let qualified_names: Vec<&str> = idents
            .iter()
            .filter_map(|i| i.as_ident().map(|ident| ident.name.as_str()))
            .collect();

        if qualified_names.is_empty() {
            return None;
        }

        tracing::trace!("resolving scoped ident with '{:?}'", qualified_names);

        // Use lookup_qualified to resolve the full path
        let kind_filters = kinds_filter.unwrap_or_default();
        let symbols = self
            .ty
            .scopes
            .lookup_qualified(&qualified_names, kind_filters)?;

        tracing::trace!(
            "found {:?} symbols for scoped ident '{:?}'",
            symbols
                .iter()
                .map(|s| s.format(Some(self.ty.unit.interner())))
                .collect::<Vec<_>>(),
            qualified_names
        );

        // Use the last matching symbol
        let symbol = symbols.last().copied()?;
        if symbol.kind() == SymKind::TypeAlias {
            // Get the type of the resolved symbol
            let type_id = symbol.type_of()?;
            self.ty.unit.opt_get_symbol(type_id)
        } else {
            Some(symbol)
        }
    }

    fn infer_struct_expression(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        node.child_by_field(*self.ty.unit, LangRust::field_name)
            .or_else(|| node.child_by_field(*self.ty.unit, LangRust::field_type))
            .and_then(|ty_node| self.infer_expr_no_filter(&ty_node))
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

        self.infer_expr_no_filter(last_child)
    }

    fn infer_binary_expression(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let (left_node, _, outcome) = self.get_binary_components(node)?;

        match outcome {
            Some(BinaryOperatorOutcome::ReturnsBool) => self.primitive_type("bool"),
            Some(BinaryOperatorOutcome::ReturnsLeftOperand) => {
                self.infer_expr_no_filter(&left_node)
            }
            None => None,
        }
    }

    fn infer_field_expression(
        &mut self,
        node: &HirNode<'tcx>,
        kinds_filter: Option<Vec<SymKind>>,
    ) -> Option<&'tcx Symbol> {
        let value_node = node.child_by_field(*self.ty.unit, LangRust::field_value)?;
        let obj_type = self.ty.infer_expr(&value_node)?;

        let field_node = node.child_by_field(*self.ty.unit, LangRust::field_field)?;
        let field_ident = field_node.as_ident()?;

        // Try to find field with kind filtering
        let field_symbol = if let Some(ref kinds) = kinds_filter {
            // For callable resolution, look for function members
            if kinds.contains(&SymKind::Function) {
                self.ty.scopes.lookup_member_symbol(
                    obj_type,
                    &field_ident.name,
                    Some(SymKind::Function),
                )
            } else {
                None
            }
        } else {
            // For type resolution, try field first, then function
            self.ty
                .scopes
                .lookup_member_symbol(obj_type, &field_ident.name, Some(SymKind::Field))
                .or_else(|| {
                    self.ty.scopes.lookup_member_symbol(
                        obj_type,
                        &field_ident.name,
                        Some(SymKind::Function),
                    )
                })
        }?;

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
        self.ty.infer_expr(&child)
    }

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

    fn collect_types(&mut self, node: &HirNode<'tcx>, collected_symbols: &mut Vec<&'tcx Symbol>) {
        if let Some(type_symbol) = self.ty.resolve_type(node)
            && !collected_symbols.iter().any(|s| s.id() == type_symbol.id())
        {
            collected_symbols.push(type_symbol);
        }

        let children = node.children(self.ty.unit);
        for child_node in children {
            if !Self::is_trivia(&child_node) {
                self.collect_types(&child_node, collected_symbols);
            }
        }
    }

    /// Collects all child nodes that are identifiers
    fn collect_ident_recursive(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        identifiers: &mut Vec<HirNode<'tcx>>,
    ) {
        // Check if current node is an identifier kind
        if node.kind_id() == LangRust::identifier {
            identifiers.push(*node);
        }

        // Recursively check children
        let children = node.children(unit);
        for child_node in children {
            if !Self::is_trivia(&child_node) {
                Self::collect_ident_recursive(unit, &child_node, identifiers);
            }
        }
    }

    fn is_identifier_kind(kind_id: u16) -> bool {
        matches!(
            kind_id,
            LangRust::identifier
                | LangRust::scoped_identifier
                | LangRust::field_identifier
                | LangRust::type_identifier
        )
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
