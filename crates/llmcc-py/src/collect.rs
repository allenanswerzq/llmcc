use std::collections::HashMap;

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirIdent, HirNode, HirScope};
use llmcc_core::next_hir_id;
use llmcc_core::scope::{Scope, ScopeStack};
use llmcc_core::symbol::{ScopeId, SymKind, Symbol};
use llmcc_resolver::{CollectorScopes, ResolverOption};

use crate::LangPython;
use crate::token::AstVisitorPython;

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
        namespace: &'tcx Scope<'tcx>,
    ) {
        ident.set_symbol(sym);
        sn.set_ident(ident);

        let depth = scopes.scope_depth();
        let scope = if sym.opt_scope().is_none() {
            self.alloc_scope(unit, sym)
        } else {
            self.get_scope(sym.scope())
                .unwrap_or_else(|| self.alloc_scope(unit, sym))
        };

        sym.set_scope(scope.id());
        sn.set_scope(scope);
        scope.add_parent(namespace);

        scopes.push_scope(scope);
        self.visit_children(unit, node, scopes, scope, Some(sym));
        scopes.pop_until(depth);
    }

    fn mark_global_if_top_level(&self, sym: &'tcx Symbol, scopes: &CollectorScopes<'tcx>, parent: Option<&Symbol>) {
        if parent.is_none() {
            sym.set_is_global(true);
            scopes.globals().insert(sym);
            return;
        }

        if let Some(parent_sym) = parent
            && matches!(parent_sym.kind(), SymKind::File | SymKind::Module | SymKind::Crate)
        {
            sym.set_is_global(true);
            scopes.globals().insert(sym);
        }
    }
}

impl<'tcx> AstVisitorPython<'tcx, CollectorScopes<'tcx>> for CollectorVisitor<'tcx> {
    fn visit_module(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let depth = scopes.scope_depth();
        let meta = unit.unit_meta();

        // Track package scope for parent relationships
        let mut package_scope: Option<&'tcx Scope<'tcx>> = None;

        // Set up package (crate) scope from unit metadata
        if let Some(ref package_name) = meta.package_name
            && let Some(symbol) = scopes.lookup_or_insert_global(package_name, node, SymKind::Crate)
        {
            scopes.push_scope_with(node, Some(symbol));
            package_scope = scopes.top();
        }

        // For files in subdirectories, create a module scope for proper hierarchy traversal
        let mut module_wrapper_scope: Option<&'tcx Scope<'tcx>> = None;
        if let Some(ref module_name) = meta.module_name
            && let Some(module_sym) = scopes.lookup_or_insert_global(module_name, node, SymKind::Module)
        {
            let mod_scope = self.alloc_scope(unit, module_sym);
            if let Some(pkg_s) = package_scope {
                mod_scope.add_parent(pkg_s);
            }
            module_wrapper_scope = Some(mod_scope);
        }

        // Create file symbol and scope
        if let Some(ref file_name) = meta.file_name {
            let file_sym = scopes.lookup_or_insert(file_name, node, SymKind::File);
            if let Some(file_sym) = file_sym {
                let arena_name = unit.cc.arena().alloc_str(file_name);
                let ident = unit.cc.alloc_file_ident(next_hir_id(), arena_name, file_sym);
                ident.set_symbol(file_sym);

                let file_scope = self.alloc_scope(unit, file_sym);
                file_scope.add_parent(namespace);
                if let Some(mod_scope) = module_wrapper_scope {
                    file_scope.add_parent(mod_scope);
                }
                if let Some(pkg_s) = package_scope {
                    file_scope.add_parent(pkg_s);
                }

                if let Some(sn) = node.as_scope() {
                    sn.set_ident(ident);
                    sn.set_scope(file_scope);
                }

                scopes.push_scope(file_scope);
                self.visit_children(unit, node, scopes, file_scope, Some(file_sym));
                scopes.pop_until(depth);
                return;
            }
        }

        // Fallback: just visit children
        self.visit_children(unit, node, scopes, namespace, None);
        scopes.pop_until(depth);
    }

    fn visit_class_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let ident = node
            .ident_by_field(unit, LangPython::field_name)
            .or_else(|| node.find_ident(unit));
        let Some(ident) = ident else {
            self.visit_children(unit, node, scopes, namespace, parent);
            return;
        };

        let sym = scopes.lookup_or_insert(ident.name, node, SymKind::Struct);
        let Some(sym) = sym else {
            self.visit_children(unit, node, scopes, namespace, parent);
            return;
        };

        self.mark_global_if_top_level(sym, scopes, parent);

        if let Some(sn) = node.as_scope() {
            self.visit_with_scope(unit, node, scopes, sym, sn, ident, namespace);
        } else {
            ident.set_symbol(sym);
            self.visit_children(unit, node, scopes, namespace, Some(sym));
        }
    }

    fn visit_function_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_function_like(unit, node, scopes, namespace, parent);
    }
}

impl<'tcx> CollectorVisitor<'tcx> {
    fn visit_function_like(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let ident = node
            .ident_by_field(unit, LangPython::field_name)
            .or_else(|| node.find_ident(unit));
        let Some(ident) = ident else {
            self.visit_children(unit, node, scopes, namespace, parent);
            return;
        };

        let kind = if parent.is_some_and(|p| matches!(p.kind(), SymKind::Struct)) {
            SymKind::Method
        } else {
            SymKind::Function
        };

        let sym = scopes.lookup_or_insert(ident.name, node, kind);
        let Some(sym) = sym else {
            self.visit_children(unit, node, scopes, namespace, parent);
            return;
        };

        self.mark_global_if_top_level(sym, scopes, parent);

        if let Some(sn) = node.as_scope() {
            self.visit_with_scope(unit, node, scopes, sym, sn, ident, namespace);
        } else {
            ident.set_symbol(sym);
            self.visit_children(unit, node, scopes, namespace, Some(sym));
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
    let unit_globals_val = Scope::new(llmcc_core::ir::HirId(unit.index));
    let scope_id = unit_globals_val.id().0;
    let unit_globals = arena.alloc_with_id(scope_id, unit_globals_val);
    let mut scopes = CollectorScopes::new(cc, unit.index, scope_stack, unit_globals);

    let mut visit = CollectorVisitor::new();
    visit.visit_node(&unit, node, &mut scopes, unit_globals, None);

    unit_globals
}
