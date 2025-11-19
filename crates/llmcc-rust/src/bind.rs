use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_resolver::{BinderScopes, ResolverOption};

use crate::token::AstVisitorRust;
use crate::token::LangRust;
use crate::util::{parse_crate_name, parse_file_name, parse_module_name};

/// Visitor for resolving symbol bindings and establishing relationships.
#[derive(Debug)]
pub struct BinderVisitor<'tcx> {
    phantom: std::marker::PhantomData<&'tcx ()>,
}

impl<'tcx> BinderVisitor<'tcx> {
    fn new() -> Self {
        Self {
            phantom: std::marker::PhantomData,
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

    fn symbol_from_field(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &BinderScopes<'tcx>,
        field_id: u16,
    ) -> Option<&'tcx Symbol> {
        if let Some(ident) = node.as_ident() {
            if let Some(existing) = scopes.lookup_symbol_with(&ident.name) {
                return Some(existing);
            }
            return Some(ident.symbol());
        }
        let ident = node.child_identifier_by_field(*unit, field_id)?;
        if let Some(existing) = scopes.lookup_symbol_with(&ident.name) {
            return Some(existing);
        }
        Some(ident.symbol())
    }

    fn identifier_name(unit: &CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<String> {
        if let Some(ident) = node.as_ident() {
            return Some(Self::normalize_identifier(&ident.name));
        }
        if let Some(ident) = node.find_identifier(*unit) {
            return Some(Self::normalize_identifier(&ident.name));
        }
        None
    }

    fn normalize_identifier(name: &str) -> String {
        name.rsplit("::").next().unwrap_or(name).to_string()
    }

    fn first_child_node(unit: &CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<HirNode<'tcx>> {
        let child_id = node.children().first()?;
        Some(unit.hir_node(*child_id))
    }

    fn lookup_symbol_in_stack<'a>(
        scopes: &'a BinderScopes<'tcx>,
        name: &str,
    ) -> Option<&'tcx Symbol> {
        if name.is_empty() {
            return None;
        }
        let name_key = scopes.interner().intern(name);
        for scope in scopes.scopes().iter().rev() {
            let matches = scope.lookup_symbols(name_key);
            if let Some(symbol) = matches.last() {
                return Some(symbol);
            }
        }
        None
    }

    fn lookup_callable_symbol(
        &self,
        unit: &CompileUnit<'tcx>,
        scopes: &BinderScopes<'tcx>,
        name: &str,
    ) -> Option<&'tcx Symbol> {
        if let Some(symbol) = scopes.lookup_symbol_with(name)
            && matches!(symbol.kind(), SymKind::Function | SymKind::Macro)
        {
            return Some(symbol);
        }

        // Fallback to global scan for callable symbols
        let name_key = unit.interner().intern(name);
        let map = unit.cc.symbol_map.read();
        map.values().copied().find(|symbol| {
            symbol.name == name_key && matches!(symbol.kind(), SymKind::Function | SymKind::Macro)
        })
    }

    fn resolve_expression_symbol(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &BinderScopes<'tcx>,
    ) -> Option<&'tcx Symbol> {
        match node.kind_id() {
            kind if Self::is_identifier_kind(kind) => {
                let name = Self::identifier_name(unit, node)?;
                self.lookup_callable_symbol(unit, scopes, &name)
            }
            kind if kind == LangRust::field_expression => {
                let field = node.child_by_field(*unit, LangRust::field_field)?;
                let name = Self::identifier_name(unit, &field)?;
                self.lookup_callable_symbol(unit, scopes, &name)
            }
            kind if kind == LangRust::reference_expression => {
                let value = node.child_by_field(*unit, LangRust::field_value)?;
                self.resolve_expression_symbol(unit, &value, scopes)
            }
            kind if kind == LangRust::call_expression => {
                let inner = node.child_by_field(*unit, LangRust::field_function)?;
                self.resolve_expression_symbol(unit, &inner, scopes)
            }
            kind if kind == LangRust::await_expression
                || kind == LangRust::try_expression
                || kind == LangRust::parenthesized_expression =>
            {
                let child = Self::first_child_node(unit, node)?;
                self.resolve_expression_symbol(unit, &child, scopes)
            }
            _ => {
                let name = Self::identifier_name(unit, node)?;
                self.lookup_callable_symbol(unit, scopes, &name)
            }
        }
    }

    fn resolve_macro_symbol(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &BinderScopes<'tcx>,
    ) -> Option<&'tcx Symbol> {
        let macro_node = node.child_by_field(*unit, LangRust::field_macro)?;
        let name = Self::identifier_name(unit, &macro_node)?;
        self.lookup_callable_symbol(unit, scopes, &name)
    }

    fn resolve_call_target(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &BinderScopes<'tcx>,
    ) -> Option<&'tcx Symbol> {
        let function = node.child_by_field(*unit, LangRust::field_function)?;
        self.resolve_expression_symbol(unit, &function, scopes)
    }

    fn resolve_type_from_node(
        unit: &CompileUnit<'tcx>,
        type_node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
    ) -> Option<&'tcx Symbol> {
        let ident = type_node.find_identifier(*unit)?;

        if let Some(existing) = Self::lookup_symbol_in_stack(scopes, &ident.name) {
            return Some(existing);
        }

        scopes.lookup_or_insert_global(&ident.name, type_node, SymKind::Type)
    }

    fn link_symbol_with_type(symbol: &Symbol, ty: &Symbol) {
        if symbol.type_of().is_none() {
            symbol.set_type_of(ty.id());
        }
        symbol.add_dependency(ty);
    }

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
            if let Some(target) = Self::lookup_symbol_in_stack(scopes, &name) {
                symbol.add_dependency(target);
                if let Some(owner) = owner {
                    owner.add_dependency(target);
                }
            }
        };
        Self::visit_type_identifiers(unit, type_node, &mut visit);
    }

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

    fn push_scope_node(scopes: &mut BinderScopes<'tcx>, sn: &'tcx llmcc_core::ir::HirScope<'tcx>) {
        if sn.opt_ident().is_some() {
            scopes.push_scope_recursive(sn.scope().id());
        } else {
            scopes.push_scope(sn.scope().id());
        }
    }

    /// Infer type from expressions (like C#'s GetTypeFromNode)
    /// Returns the type symbol for an expression node
    fn infer_type_from_expr(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &BinderScopes<'tcx>,
    ) -> Option<&'tcx Symbol> {
        match node.kind() {
            // === Identifiers - look up type in scope stack ===
            HirKind::Identifier => {
                node.as_ident().and_then(|ident| {
                    Self::lookup_symbol_in_stack(scopes, &ident.name).and_then(|symbol| {
                        // If this identifier refers to a function/value, get its type
                        if let Some(type_id) = symbol.type_of() {
                            unit.opt_get_symbol(type_id)
                        } else {
                            Some(symbol)
                        }
                    })
                })
            }

            // === Binary Operations - type depends on operator ===
            HirKind::Internal => {
                let children: Vec<_> = node.children().iter().cloned().collect();

                // Binary operations (3+ children: left, operator, right)
                if children.len() >= 3 {
                    let op_node = unit.hir_node(children[1]);
                    if let Some(op_text) = op_node.as_text() {
                        let operator = &op_text.text;

                        // Comparison operators always return bool
                        if matches!(
                            operator.as_str(),
                            "==" | "!=" | "<" | ">" | "<=" | ">=" | "&&" | "||"
                        ) {
                            // Return bool type symbol if it exists
                            return scopes.lookup_or_insert_global(
                                "bool",
                                &unit.hir_node(children[0]),
                                SymKind::Type,
                            );
                        }

                        // Arithmetic operators return left operand type
                        if matches!(operator.as_str(), "+" | "-" | "*" | "/" | "%") {
                            let left_node = unit.hir_node(children[0]);
                            return Self::infer_type_from_expr(unit, &left_node, scopes);
                        }
                    }
                }

                // Fall back to first meaningful child's type
                for child_id in &children {
                    let child = unit.hir_node(*child_id);
                    if !matches!(child.kind(), HirKind::Text | HirKind::Comment) {
                        if let Some(ty) = Self::infer_type_from_expr(unit, &child, scopes) {
                            return Some(ty);
                        }
                    }
                }
                None
            }

            // === Other kinds - recursive traversal to first child ===
            _ => node.children().first().and_then(|child_id| {
                Self::infer_type_from_expr(unit, &unit.hir_node(*child_id), scopes)
            }),
        }
    }
    /// Infer return type from block (last expression or void)
    fn infer_block_type(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &BinderScopes<'tcx>,
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
        if let Some(scope_id) = parse_crate_name(file_path)
            .and_then(|crate_name| {
                scopes.lookup_or_insert_global(&crate_name, node, SymKind::Crate)
            })
            .and_then(|symbol| symbol.scope())
        {
            scopes.push_scope(scope_id);
        }

        if let Some(scope_id) = parse_module_name(file_path).and_then(|module_name| {
            scopes
                .lookup_or_insert_global(&module_name, node, SymKind::Module)
                .and_then(|symbol| symbol.scope())
        }) {
            scopes.push_scope(scope_id);
        }

        if let Some(scope_id) = parse_file_name(file_path).and_then(|file_name| {
            scopes
                .lookup_or_insert(&file_name, node, SymKind::File)
                .and_then(|symbol| symbol.scope())
        }) {
            scopes.push_scope(scope_id);
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
            .and_then(|ret_ty| Self::resolve_type_from_node(unit, ret_ty, scopes))
            .unwrap_or_else(|| {
                // Default to void/unit type if no return type found
                scopes
                    .lookup_or_insert_global("void_fn", node, SymKind::Type)
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
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            if let Some(ident) = sn.opt_ident() {
                if let Some(type_node) = node.child_by_field(*unit, LangRust::field_type) {
                    if let Some(target) = Self::resolve_type_from_node(unit, &type_node, scopes) {
                        Self::link_symbol_with_type(ident.symbol(), target);
                    }
                    Self::link_type_references(unit, &type_node, scopes, ident.symbol(), None);
                }
            }

            let depth = scopes.scope_depth();
            let child_parent = sn.opt_ident().map(|ident| ident.symbol()).or(parent);
            Self::push_scope_node(scopes, sn);
            self.visit_children(unit, node, scopes, namespace, child_parent);
            scopes.pop_until(depth);
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
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
            && let Some(callee) = self.resolve_call_target(unit, node, scopes)
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
}

pub fn bind_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    namespace: &'tcx Scope<'tcx>,
    _config: &ResolverOption,
) {
    BinderVisitor::new().visit_node(&unit, node, scopes, namespace, None);
}
