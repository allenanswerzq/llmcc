use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_resolver::{BinderScopes, ResolverOption};

use crate::token::AstVisitorRust;
use crate::token::LangRust;
use crate::util::{parse_crate_name, parse_file_name, parse_module_name};

use super::resolution::SymbolResolver;
use super::inference::TypeInferrer;
use super::linker::SymbolLinker;

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
        let file_path = unit.file_path().unwrap();
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
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if node.child_by_field(*unit, LangRust::field_body).is_none() {
            return;
        }

        let Some(sn) = node.as_scope() else {
            self.visit_children(unit, node, scopes, namespace, parent);
            return;
        };

        let module_symbol = sn.opt_ident().and_then(|ident| ident.opt_symbol());

        let depth = scopes.scope_depth();
        scopes.push_scope_node(sn);
        self.visit_children(unit, node, scopes, namespace, module_symbol.or(parent));
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
        let ret_node = node.child_by_field(*unit, LangRust::field_return_type);
        if let Some(ret_ty) = ret_node {
            self.visit_node(unit, &ret_ty, scopes, namespace, parent);
        }
        let sn = node.as_scope().unwrap();
        let ty = ret_node
            .as_ref()
            .and_then(|ret_ty| {
                if let Some(ident) = ret_ty.find_identifier(*unit)
                    && ident.name == "Self"
                    && let Some(p) = parent
                {
                    return Some(p);
                }
                let mut resolver = SymbolResolver::new(unit, scopes);
                resolver.resolve_type_from_node(ret_ty)
            })
            .unwrap_or_else(|| {
                scopes
                    .lookup_or_insert_global("void", node, SymKind::Primitive).unwrap()
            });

        let func_symbol = sn.opt_ident().and_then(|ident| ident.opt_symbol());
        if let Some(symbol) = func_symbol {
            if symbol.type_of().is_none() {
                symbol.set_type_of(ty.id());
            }
            symbol.add_dependency(ty);
            if let Some(ret_ty) = ret_node.as_ref() {
                let mut linker = SymbolLinker::new(unit, scopes);
                linker.link_type_references(ret_ty, symbol, None);
            }
        }

        let depth = scopes.scope_depth();
        let child_parent = func_symbol.or(parent);
        scopes.push_scope_node(sn);
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
        let sn = node.as_scope().unwrap();
        let depth = scopes.scope_depth();
        let struct_symbol = sn.opt_ident().and_then(|ident| ident.opt_symbol());

        if let Some(parent_sym) = parent
            && let Some(struct_sym) = struct_symbol
        {
            parent_sym.add_dependency(struct_sym);
        }

        if let Some(struct_sym) = struct_symbol {
            for child_id in node.children() {
                let child = unit.hir_node(*child_id);
                if child.kind_id() == LangRust::where_clause {
                    let mut linker = SymbolLinker::new(unit, scopes);
                    linker.link_where_clause_dependencies(&child, struct_sym);
                }
            }
        }

        let child_parent = struct_symbol.or(parent);
        scopes.push_scope_node(sn);
        self.visit_children(unit, node, scopes, namespace, child_parent);
        scopes.pop_until(depth);
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
            let child_parent = sn
                .opt_ident()
                .and_then(|ident| ident.opt_symbol())
                .or(parent);
            scopes.push_scope_node(sn);
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
            let child_parent = sn
                .opt_ident()
                .and_then(|ident| ident.opt_symbol())
                .or(parent);
            scopes.push_scope_node(sn);
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

        if let Some(type_node_ref) = &type_node {
            // Resolve target type
            let (target, scope_id) = {
                let mut resolver = SymbolResolver::new(unit, scopes);
                if let Some(target) = resolver.resolve_type_from_node(type_node_ref)
                    && let Some(scope_id) = target.scope()
                {
                    (target, scope_id)
                } else {
                    return;
                }
            };

            // Resolve trait symbol (if any)
            let trait_info = if let Some(trait_node) = node.child_by_field(*unit, LangRust::field_trait) {
                let mut resolver = SymbolResolver::new(unit, scopes);
                resolver.resolve_type_from_node(&trait_node).map(|sym| (trait_node, sym))
            } else {
                None
            };

            let mut linker = SymbolLinker::new(unit, scopes);
            linker.link_type_references(type_node_ref, target, None);

            if let Some((trait_node, trait_symbol)) = trait_info {
                if let Some(trait_scope_id) = trait_symbol.scope() {
                    let struct_scope = unit.cc.get_scope(scope_id);
                    let trait_scope = unit.cc.get_scope(trait_scope_id);
                    struct_scope.add_parent(trait_scope);

                    linker.link_type_references(&trait_node, target, None);
                }
            }

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
        let resolver = SymbolResolver::new(unit, scopes);
        if let Some(symbol) = resolver.symbol_from_field(node, LangRust::field_name) {
            let mut linker = SymbolLinker::new(unit, scopes);
            linker.set_symbol_type_from_field(
                node,
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
            scopes.push_scope_node(sn);
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
        if let Some(caller) = parent {
            let mut resolver = SymbolResolver::new(unit, scopes);
            if let Some(target) = resolver.resolve_macro_symbol(node) {
                caller.add_dependency(target);
            }
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
        let resolver = SymbolResolver::new(unit, scopes);
        if let Some(symbol) = resolver.symbol_from_field(node, LangRust::field_name) {
            let mut linker = SymbolLinker::new(unit, scopes);
            linker.set_symbol_type_from_field(
                node,
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
        let resolver = SymbolResolver::new(unit, scopes);
        if let Some(symbol) = resolver.symbol_from_field(node, LangRust::field_name) {
            let mut linker = SymbolLinker::new(unit, scopes);
            linker.set_symbol_type_from_field(
                node,
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
        if let Some(caller) = parent {
            let mut inferrer = TypeInferrer::new(unit, scopes);
            if let Some(callee) = inferrer.resolve_call_target(node, Some(caller)) {
                caller.add_dependency(callee);
            }
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
        let resolver = SymbolResolver::new(unit, scopes);
        if let Some(symbol) = resolver.symbol_from_field(node, LangRust::field_name) {
            let mut linker = SymbolLinker::new(unit, scopes);
            linker.set_symbol_type_from_field(
                node,
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
        let resolver = SymbolResolver::new(unit, scopes);
        if let Some(symbol) = resolver.symbol_from_field(node, LangRust::field_name) {
            let mut linker = SymbolLinker::new(unit, scopes);
            linker.set_symbol_type_from_field(
                node,
                symbol,
                parent,
                LangRust::field_default_type,
            );
            linker.link_trait_bounds(node, symbol, parent);
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_const_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let resolver = SymbolResolver::new(unit, scopes);
        if let Some(symbol) = resolver.symbol_from_field(node, LangRust::field_name) {
            let mut linker = SymbolLinker::new(unit, scopes);
            linker.set_symbol_type_from_field(
                node,
                symbol,
                parent,
                LangRust::field_type,
            );
            if let Some(owner) = parent {
                owner.add_dependency(symbol);
            }
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
        let resolver = SymbolResolver::new(unit, scopes);
        if let Some(symbol) = resolver.symbol_from_field(node, LangRust::field_name) {
            let mut linker = SymbolLinker::new(unit, scopes);
            linker.set_symbol_type_from_field(
                node,
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
        let resolver = SymbolResolver::new(unit, scopes);
        if let Some(symbol) = resolver.symbol_from_field(node, LangRust::field_name) {
            let mut linker = SymbolLinker::new(unit, scopes);
            linker.set_symbol_type_from_field(
                node,
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
        let resolver = SymbolResolver::new(unit, scopes);
        if let Some(symbol) = resolver.symbol_from_field(node, LangRust::field_name) {
            let has_direct_value = node.child_by_field(*unit, LangRust::field_value).is_some();
            let mut linker = SymbolLinker::new(unit, scopes);
            linker.set_symbol_type_from_field(
                node,
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
                    linker.link_type_references(&child, symbol, parent);
                }
            }
        } else if let Some(type_node) = node.child_by_field(*unit, LangRust::field_value)
            && let Some(owner_symbol) = parent
        {
            let mut linker = SymbolLinker::new(unit, scopes);
            linker.link_type_references(&type_node, owner_symbol, None);
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
        let resolver = SymbolResolver::new(unit, scopes);
        if let Some(symbol) = resolver.symbol_from_field(node, LangRust::field_pattern) {
            let mut linker = SymbolLinker::new(unit, scopes);
            linker.set_symbol_type_from_field(
                node,
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
            let mut resolver = SymbolResolver::new(unit, scopes);
            type_symbol = resolver.resolve_type_from_node(&type_node);
        }

        // If no explicit type, try to infer from value
        if type_symbol.is_none()
            && let Some(value_node) = node.child_by_field(*unit, LangRust::field_value)
        {
            let mut inferrer = TypeInferrer::new(unit, scopes);
            type_symbol = inferrer.infer_type_from_expr(&value_node);
        }

        // Assign type to pattern
        if let Some(ty) = type_symbol {
            if let Some(pattern) = node.child_by_field(*unit, LangRust::field_pattern) {
                let mut linker = SymbolLinker::new(unit, scopes);
                linker.assign_type_to_pattern(&pattern, ty);

                if let Some(type_node) = node.child_by_field(*unit, LangRust::field_type) {
                    linker.link_pattern_type_references(&pattern, &type_node, parent);
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
                scopes.push_scope_node(sn);
                self.visit_children(unit, node, scopes, namespace, parent);
                scopes.pop_until(depth);
            } else {
                self.visit_children(unit, node, scopes, namespace, parent);
            }
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    fn visit_struct_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(name_node) = node
            .child_by_field(*unit, LangRust::field_name)
            .or_else(|| node.child_by_field(*unit, LangRust::field_type))
        {
            let mut resolver = SymbolResolver::new(unit, scopes);
            if name_node.kind_id() == LangRust::scoped_type_identifier
                || name_node.kind_id() == LangRust::scoped_identifier
            {
                if let Some(sym) =
                    resolver.resolve_scoped_identifier_symbol(&name_node, parent)
                {
                    if let Some(caller) = parent {
                        caller.add_dependency(sym);
                    }
                }
            } else if let Some(name) = resolver.identifier_name(&name_node) {
                if let Some(sym) = scopes.lookup_symbol(&name) {
                    if let Some(caller) = parent {
                        caller.add_dependency(sym);
                    }
                }
            }
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

    fn dependency_names(cc: &CompileCtxt<'_>, sym_id: SymId) -> Vec<String> {
        let map = cc.symbol_map.read();
        let symbol = map
            .get(&sym_id)
            .copied()
            .unwrap_or_else(|| panic!("missing symbol for id {:?}", sym_id));
        let deps = symbol.depends.read().clone();
        let mut names = Vec::new();
        for dep in deps {
            if let Some(target) = map.get(&dep) {
                let fqn_key = target.fqn();
                if let Some(fqn) = cc.interner.resolve_owned(fqn_key) {
                    names.push(fqn);
                }
            }
        }
        names.sort();
        names
    }

    fn assert_dependencies(source: &[&str], expectations: &[(&str, SymKind, &[&str])]) {
        with_compiled_unit(source, |cc| {
            for (name, kind, deps) in expectations {
                let sym_id = find_symbol_id(cc, name, *kind);
                let actual = dependency_names(cc, sym_id);
                let expected: Vec<String> = deps.iter().map(|s| s.to_string()).collect();

                let mut missing = Vec::new();
                for expected_dep in &expected {
                    if !actual.iter().any(|actual_dep| actual_dep == expected_dep) {
                        missing.push(expected_dep.clone());
                    }
                }

                assert!(
                    missing.is_empty(),
                    "dependency mismatch for symbol {name}: expected suffixes {:?}, actual FQNs {:?}, missing {:?}",
                    expected,
                    actual,
                    missing
                );
            }
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

    #[test]
    fn test_trait_default_method_resolution() {
        let source = r#"
trait Greeter {
    fn greet() {}
}

struct Foo;
impl Greeter for Foo {}

fn run() {
    let f = Foo;
    f.greet();
}
"#;
        assert_dependencies(
            &[source],
            &[(
                "run",
                SymKind::Function,
                &[
                    "_c::_m::source_0::Foo",
                    "_c::_m::source_0::Greeter::greet", // Should resolve to trait method
                ],
            )],
        );
    }
}
