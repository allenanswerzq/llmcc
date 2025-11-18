use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_resolver::BinderScopes;

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

    fn symbol_from_field(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        field_id: u16,
    ) -> Option<&'tcx Symbol> {
        let ident_id = node.find_identifier_for_field(*unit, field_id)?;
        let ident = unit.hir_node(ident_id).as_ident()?;
        Some(ident.symbol())
    }

    fn resolve_type_from_field(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        field_id: u16,
    ) -> Option<&'tcx Symbol> {
        let type_id = node.find_identifier_for_field(*unit, field_id)?;
        let ty_node = unit.hir_node(type_id);
        let ident = ty_node.as_ident()?;

        if let Some(existing) = Self::lookup_symbol_in_stack(scopes, &ident.name) {
            return Some(existing);
        }

        scopes.lookup_or_insert_global(&ident.name, &ty_node, SymKind::Type)
    }

    fn link_symbol_with_type(symbol: &Symbol, ty: &Symbol) {
        if symbol.type_of().is_none() {
            symbol.set_type_of(ty.id());
        }
        symbol.add_dependency(ty);
    }

    fn set_symbol_type_from_field(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        symbol: &Symbol,
        field_id: u16,
    ) {
        if let Some(ty) = Self::resolve_type_from_field(unit, node, scopes, field_id) {
            Self::link_symbol_with_type(symbol, ty);
        }
    }

    fn push_scope_node(scopes: &mut BinderScopes<'tcx>, sn: &'tcx llmcc_core::ir::HirScope<'tcx>) {
        if sn.opt_ident().is_some() {
            scopes.push_scope_recursive(sn.scope().id());
        } else {
            scopes.push_scope(sn.scope().id());
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
        let ty = node
            .find_identifier_for_field(*unit, LangRust::field_return_type)
            .and_then(|ty_id| {
                let ty_node = unit.hir_node(ty_id);
                scopes.lookup_or_insert_global(
                    &ty_node.as_ident().unwrap().name,
                    &ty_node,
                    SymKind::Type,
                )
            })
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
        }

        let depth = scopes.scope_depth();
        Self::push_scope_node(scopes, sn);
        self.visit_children(unit, node, scopes, namespace, parent);
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
            Self::push_scope_node(scopes, sn);
            self.visit_children(unit, node, scopes, namespace, parent);
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
            Self::push_scope_node(scopes, sn);
            self.visit_children(unit, node, scopes, namespace, parent);
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
            Self::push_scope_node(scopes, sn);
            self.visit_children(unit, node, scopes, namespace, parent);
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
                if let Some(target) =
                    Self::resolve_type_from_field(unit, node, scopes, LangRust::field_type)
                {
                    Self::link_symbol_with_type(ident.symbol(), target);
                }
            }

            let depth = scopes.scope_depth();
            Self::push_scope_node(scopes, sn);
            self.visit_children(unit, node, scopes, namespace, parent);
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
        if let Some(symbol) = Self::symbol_from_field(unit, node, LangRust::field_name) {
            if let Some(ret_ty) =
                Self::resolve_type_from_field(unit, node, scopes, LangRust::field_return_type)
            {
                Self::link_symbol_with_type(symbol, ret_ty);
            }
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

    fn visit_const_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(symbol) = Self::symbol_from_field(unit, node, LangRust::field_name) {
            self.set_symbol_type_from_field(unit, node, scopes, symbol, LangRust::field_type);
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
        if let Some(symbol) = Self::symbol_from_field(unit, node, LangRust::field_name) {
            self.set_symbol_type_from_field(unit, node, scopes, symbol, LangRust::field_type);
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
        if let Some(symbol) = Self::symbol_from_field(unit, node, LangRust::field_name) {
            self.set_symbol_type_from_field(unit, node, scopes, symbol, LangRust::field_type);
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
        if let Some(symbol) = Self::symbol_from_field(unit, node, LangRust::field_name) {
            self.set_symbol_type_from_field(
                unit,
                node,
                scopes,
                symbol,
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
        if let Some(symbol) = Self::symbol_from_field(unit, node, LangRust::field_name) {
            self.set_symbol_type_from_field(
                unit,
                node,
                scopes,
                symbol,
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
        if let Some(symbol) = Self::symbol_from_field(unit, node, LangRust::field_name) {
            self.set_symbol_type_from_field(unit, node, scopes, symbol, LangRust::field_type);
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
        if let Some(symbol) = Self::symbol_from_field(unit, node, LangRust::field_pattern) {
            self.set_symbol_type_from_field(unit, node, scopes, symbol, LangRust::field_type);
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
        if let Some(symbol) = Self::symbol_from_field(unit, node, LangRust::field_pattern) {
            self.set_symbol_type_from_field(unit, node, scopes, symbol, LangRust::field_type);
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }
}

pub fn bind_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    namespace: &'tcx Scope<'tcx>,
) {
    BinderVisitor::new().visit_node(&unit, node, scopes, namespace, None);
}
