use std::collections::HashMap;

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirId, HirIdent, HirNode, HirScope};
use llmcc_core::next_hir_id;
use llmcc_core::scope::{LookupOptions, Scope, ScopeStack};
use llmcc_core::symbol::{ScopeId, SymKind, SymKindSet, Symbol};
use llmcc_resolver::{CollectorScopes, ResolverOption};

use crate::LangGo;
use crate::token::AstVisitorGo;

#[derive(Debug)]
pub struct CollectorVisitor<'tcx> {
    scope_map: HashMap<ScopeId, &'tcx Scope<'tcx>>,
}

impl<'tcx> CollectorVisitor<'tcx> {
    fn new() -> Self {
        Self {
            scope_map: HashMap::new(),
        }
    }

    fn alloc_scope(&mut self, unit: &CompileUnit<'tcx>, symbol: &'tcx Symbol) -> &'tcx Scope<'tcx> {
        let scope = unit.cc.alloc_scope(symbol.owner());
        scope.set_symbol(symbol);
        self.scope_map.insert(scope.id(), scope);
        scope
    }

    fn get_scope(&self, scope_id: ScopeId) -> Option<&'tcx Scope<'tcx>> {
        self.scope_map.get(&scope_id).copied()
    }

    fn visit_with_scope(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        sym: &'tcx Symbol,
        sn: &'tcx HirScope<'tcx>,
        ident: &'tcx HirIdent<'tcx>,
    ) {
        ident.set_symbol(sym);
        sn.set_ident(ident);

        let depth = scopes.scope_depth();
        if let Some(scope_id) = sym.opt_scope()
            && let Some(scope) = self.get_scope(scope_id)
        {
            sn.set_scope(scope);
            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, Some(sym));
            scopes.pop_until(depth);
            return;
        }

        let scope = self.alloc_scope(unit, sym);
        sym.set_scope(scope.id());
        sn.set_scope(scope);
        scopes.push_scope(scope);
        self.visit_children(unit, node, scopes, scope, Some(sym));
        scopes.pop_until(depth);
    }

    fn declare_named_scope(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        kind: SymKind,
    ) -> bool {
        let Some(sn) = node.as_scope() else {
            return false;
        };
        let Some(ident) = node.ident_by_field(unit, LangGo::field_name) else {
            return false;
        };

        if let Some(sym) =
            self.lookup_or_insert_in_scope(unit, scopes, namespace, ident.name, node, kind)
        {
            self.visit_with_scope(unit, node, scopes, sym, sn, ident);
            return true;
        }

        false
    }

    fn lookup_or_insert_in_scope(
        &self,
        unit: &CompileUnit<'tcx>,
        scopes: &CollectorScopes<'tcx>,
        scope: &'tcx Scope<'tcx>,
        name: &str,
        node: &HirNode<'tcx>,
        kind: SymKind,
    ) -> Option<&'tcx Symbol> {
        let name_key = scopes.interner().intern(name);
        let lookup = LookupOptions::default().with_kind_set(SymKindSet::from_kind(kind));
        if let Some(symbols) = scope.lookup_symbols(name_key, lookup)
            && let Some(symbol) = symbols.last().copied()
        {
            return Some(symbol);
        }

        let symbol_val = Symbol::new(node.id(), name_key);
        let sym_id = symbol_val.id().0;
        let symbol = unit.cc.arena().alloc_with_id(sym_id, symbol_val);
        symbol.set_owner(node.id());
        symbol.set_kind(kind);
        symbol.set_unit_index(scopes.unit_index());
        symbol.set_crate_index(scopes.crate_index());
        symbol.add_defining(node.id());
        symbol.set_parent_scope(scope.id());
        scope.insert(symbol);
        Some(symbol)
    }

    fn declare_field_idents(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &CollectorScopes<'tcx>,
        field_id: u16,
        kind: SymKind,
    ) {
        for child in node.children(unit) {
            if child.field_id() != field_id {
                continue;
            }
            if let Some(ident) = child.find_ident(unit)
                && let Some(sym) = scopes.lookup_or_insert(ident.name, node, kind)
            {
                ident.set_symbol(sym);
            }
        }
    }

    fn declare_method_scope(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
    ) -> bool {
        let Some(sn) = node.as_scope() else {
            return false;
        };
        let Some(ident) = node.ident_by_field(unit, LangGo::field_name) else {
            return false;
        };
        let Some(sym) = scopes.lookup_or_insert(ident.name, node, SymKind::Method) else {
            return false;
        };

        self.visit_with_scope(unit, node, scopes, sym, sn, ident);
        true
    }
}

impl<'tcx> AstVisitorGo<'tcx, CollectorScopes<'tcx>> for CollectorVisitor<'tcx> {
    fn visit_source_file(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let depth = scopes.scope_depth();
        let sn = node.as_scope();
        let meta = unit.unit_meta();

        let mut package_scope = None;
        if let Some(ref package_name) = meta.package_name
            && let Some(symbol) = scopes.lookup_or_insert_global(package_name, node, SymKind::Crate)
        {
            scopes.push_scope_with(node, Some(symbol));
            package_scope = scopes.top();
        }

        let mut module_scope = None;
        if let Some(ref module_name) = meta.module_name
            && let Some(module_sym) =
                scopes.lookup_or_insert_global(module_name, node, SymKind::Module)
        {
            let scope = if let Some(scope_id) = module_sym.opt_scope() {
                unit.cc
                    .opt_get_scope(scope_id)
                    .unwrap_or_else(|| self.alloc_scope(unit, module_sym))
            } else {
                let scope = self.alloc_scope(unit, module_sym);
                module_sym.set_scope(scope.id());
                scope
            };
            if let Some(parent_scope) = package_scope {
                scope.add_parent(parent_scope);
            }
            module_scope = Some(scope);
        }

        let mut declaration_scope = module_scope.or(package_scope).unwrap_or(namespace);
        let mut go_package_scope = None;
        if let Some(scope_name) = crate::package_scope_name(meta)
            && let Some(pkg_sym) =
                scopes.lookup_or_insert_global(&scope_name, node, SymKind::Namespace)
        {
            let scope = if let Some(scope_id) = pkg_sym.opt_scope() {
                unit.cc
                    .opt_get_scope(scope_id)
                    .unwrap_or_else(|| self.alloc_scope(unit, pkg_sym))
            } else {
                let scope = self.alloc_scope(unit, pkg_sym);
                pkg_sym.set_scope(scope.id());
                scope
            };
            if let Some(parent_scope) = module_scope.or(package_scope) {
                scope.add_parent(parent_scope);
            }
            declaration_scope = scope;
            go_package_scope = Some(scope);
        }

        if let Some(ref file_name) = meta.file_name
            && let Some(file_sym) = scopes.lookup_or_insert(file_name, node, SymKind::File)
        {
            let arena_name = unit.cc.arena().alloc_str(file_name);
            let ident = unit
                .cc
                .alloc_file_ident(next_hir_id(), arena_name, file_sym);
            ident.set_symbol(file_sym);

            let file_scope = self.alloc_scope(unit, file_sym);
            file_sym.set_scope(file_scope.id());
            if let Some(scope) = package_scope {
                file_scope.add_parent(scope);
            }
            if let Some(scope) = module_scope {
                file_scope.add_parent(scope);
            }
            if let Some(scope) = go_package_scope {
                file_scope.add_parent(scope);
            }

            if let Some(sn) = sn {
                sn.set_ident(ident);
                sn.set_scope(file_scope);
            }

            scopes.push_scope(file_scope);
            self.visit_children(unit, node, scopes, declaration_scope, Some(file_sym));
            scopes.pop_until(depth);
            return;
        }

        self.visit_children(unit, node, scopes, namespace, None);
    }

    fn visit_type_spec(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let kind = match node.child_by_field(unit, LangGo::field_type) {
            Some(type_node) if type_node.kind_id() == LangGo::struct_type => SymKind::Struct,
            Some(type_node) if type_node.kind_id() == LangGo::interface_type => SymKind::Interface,
            _ => SymKind::TypeAlias,
        };

        if !self.declare_named_scope(unit, node, scopes, namespace, kind) {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    fn visit_function_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if !self.declare_named_scope(unit, node, scopes, namespace, SymKind::Function) {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    fn visit_method_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if !self.declare_method_scope(unit, node, scopes) {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    fn visit_parameter_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.declare_field_idents(unit, node, scopes, LangGo::field_name, SymKind::Variable);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_field_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.declare_field_idents(unit, node, scopes, LangGo::field_name, SymKind::Field);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_var_spec(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.declare_field_idents(unit, node, scopes, LangGo::field_name, SymKind::Variable);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_const_spec(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.declare_field_idents(unit, node, scopes, LangGo::field_name, SymKind::Const);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_block(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);
            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        }
    }
}

pub fn collect_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    scope_stack: ScopeStack<'tcx>,
    _config: &ResolverOption,
) -> &'tcx Scope<'tcx> {
    let cc = unit.cc;
    let arena = cc.arena();
    let unit_globals_val = Scope::new(HirId(unit.index));
    let scope_id = unit_globals_val.id().0;
    let unit_globals = arena.alloc_with_id(scope_id, unit_globals_val);
    let mut scopes = CollectorScopes::new(cc, unit.index, scope_stack, unit_globals);

    let mut visit = CollectorVisitor::new();
    visit.visit_node(&unit, node, &mut scopes, unit_globals, None);

    unit_globals
}
