use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirNode, HirScope};
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_resolver::{BinderScopes, ResolverOption};

use crate::token::AstVisitorRust;
use crate::token::LangRust;
use crate::util::{parse_crate_name, parse_file_name, parse_module_name};

use crate::resolve::ExprResolver;

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

    fn initialize(&self, node: &HirNode<'tcx>, scopes: &mut BinderScopes<'tcx>) {
        let primitives = [
            "i32", "i64", "i16", "i8", "i128", "isize", "u32", "u64", "u16", "u8", "u128", "usize",
            "f32", "f64", "bool", "char", "str",
        ];
        for prim in primitives {
            scopes.lookup_or_insert_global(prim, node, SymKind::Primitive);
        }
    }

    fn visit_scoped_named(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        sn: &'tcx HirScope<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let depth = scopes.scope_depth();
        let child_parent = sn
            .opt_ident()
            .and_then(|ident| ident.opt_symbol())
            .or(parent);

        scopes.push_scope_node(sn);
        self.visit_children(unit, node, scopes, scopes.top(), child_parent);
        scopes.pop_until(depth);

        if let Some(owner) = namespace.opt_symbol()
            && let Some(symbol) = sn.opt_symbol()
        {
            // namespace owner to curent relationship
            owner.add_dependency(symbol);
        }
    }

    /// Resolve through alias chains to get the canonical type symbol.
    fn resolve_canonical_type<'a>(unit: &CompileUnit<'a>, mut symbol: &'a Symbol) -> &'a Symbol {
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

    fn lookup_field_type<'a>(
        unit: &CompileUnit<'a>,
        owner: &'a Symbol,
        field_name: &str,
    ) -> Option<&'a Symbol> {
        let owner = Self::resolve_canonical_type(unit, owner);
        let scope_id = owner.opt_scope()?;
        let scope = unit.cc.get_scope(scope_id);
        let field_key = unit.cc.interner.intern(field_name);
        let field_symbol = scope.lookup_symbols(field_key)?.last().copied()?;
        field_symbol
            .type_of()
            .and_then(|ty_id| unit.opt_get_symbol(ty_id))
    }

    /// Bind a pattern (simple identifier or struct pattern) to a type
    fn bind_pattern_to_type(unit: &CompileUnit<'tcx>, pattern: &HirNode<'tcx>, ty: &'tcx Symbol) {
        if matches!(
            pattern.kind_id(),
            LangRust::type_identifier | LangRust::field_identifier
        ) {
            return;
        }

        if let Some(ident) = pattern.as_ident() {
            if let Some(sym) = ident.opt_symbol() {
                sym.set_type_of(ty.id());
                sym.add_dependency(ty);
            }
            return;
        }

        if let Some(field_ident) = pattern.child_identifier_by_field(*unit, LangRust::field_name)
            && let Some(field_ty) = Self::lookup_field_type(unit, ty, &field_ident.name)
        {
            if let Some(subpattern) = pattern.child_by_field(*unit, LangRust::field_pattern) {
                Self::bind_pattern_to_type(unit, &subpattern, field_ty);
            } else if let Some(sym) = field_ident.opt_symbol() {
                sym.set_type_of(field_ty.id());
                sym.add_dependency(field_ty);
            }
            return;
        }

        if let Some(subpattern) = pattern.child_by_field(*unit, LangRust::field_pattern) {
            Self::bind_pattern_to_type(unit, &subpattern, ty);
            return;
        }

        for child in pattern.children_nodes(unit) {
            Self::bind_pattern_to_type(unit, &child, ty);
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
                && let Some(scope_id) = symbol.opt_scope()
            {
                scopes.push_scope(scope_id);
            }
        }

        if let Some(scope_id) = parse_module_name(file_path).and_then(|module_name| {
            scopes
                .lookup_or_insert(&module_name, node, SymKind::Module)
                .and_then(|symbol| symbol.opt_scope())
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
                && let Some(scope_id) = symbol.opt_scope()
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

        let sn = node.as_scope().unwrap();
        self.visit_scoped_named(unit, node, sn, scopes, namespace, parent);
    }

    fn visit_function_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let sn = node.as_scope().unwrap();
        self.visit_scoped_named(unit, node, sn, scopes, namespace, parent);

        // At this point, all return type node should already be bound
        let ret_node = node.child_identifier_by_field(*unit, LangRust::field_return_type);
        if let Some(fn_sym) = sn.opt_symbol()
            && let Some(ret_ty) = ret_node
            && let Some(ret_sym) = ret_ty.opt_symbol()
        {
            fn_sym.set_type_of(ret_sym.id());
            fn_sym.add_dependency(ret_sym);
        }
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
        self.visit_scoped_named(unit, node, sn, scopes, namespace, parent);
    }

    fn visit_enum_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let sn = node.as_scope().unwrap();
        self.visit_scoped_named(unit, node, sn, scopes, namespace, parent);
    }

    fn visit_trait_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let sn = node.as_scope().unwrap();
        self.visit_scoped_named(unit, node, sn, scopes, namespace, parent);
    }

    fn visit_impl_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // NOTE: the impl and the define(struct/enum) reuse the same scope(namespace),
        // so, this `sn` should be the same as the scope of (struct/enum) at the collect phase we created
        let sn = node.as_scope().unwrap();
        self.visit_scoped_named(unit, node, sn, scopes, namespace, parent);

        if let Some(target_ident) = node.child_identifier_by_field(*unit, LangRust::field_type)
            && let Some(target_sym) = target_ident.opt_symbol()
            && let Some(target_scope) = target_sym.opt_scope()
            && let Some(trait_ident) = node.child_identifier_by_field(*unit, LangRust::field_trait)
            && let Some(trait_sym) = trait_ident.opt_symbol()
            && let Some(trait_scope) = trait_sym.opt_scope()
        {
            let target_scope = unit.cc.get_scope(target_scope);
            let trait_scope = unit.cc.get_scope(trait_scope);
            target_scope.add_parent(trait_scope);
            target_sym.add_dependency(trait_sym);
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
        let sn = node.as_scope().unwrap();
        self.visit_scoped_named(unit, node, sn, scopes, namespace, parent);
    }

    fn visit_macro_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let sn = node.as_scope().unwrap();
        self.visit_scoped_named(unit, node, sn, scopes, namespace, parent);
    }

    fn visit_macro_invocation(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // Get the macro name from the macro_invocation
        if let Some(macro_node) = node.child_by_field(*unit, LangRust::field_macro)
            && let Some(sym) =
                ExprResolver::new(unit, scopes).resolve_expression_symbol(&macro_node, parent)
            && let Some(ns) = namespace.opt_symbol()
        {
            ns.add_dependency(sym);
        }
    }

    fn visit_const_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        if let Some(const_ident) = node.child_identifier_by_field(*unit, LangRust::field_name)
            && let Some(const_sym) = const_ident.opt_symbol()
            && let Some(const_ty) = node.child_by_field(*unit, LangRust::field_type)
            && let Some(ty) = ExprResolver::new(unit, scopes).infer_type_from_expr(&const_ty)
        {
            const_sym.set_type_of(ty.id());
            const_sym.add_dependency(ty);
            if let Some(ns) = namespace.opt_symbol() {
                ns.add_dependency(const_sym);
            }
        }
    }

    fn visit_static_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_const_item(unit, node, scopes, namespace, parent);
    }

    fn visit_call_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(sym) = ExprResolver::new(unit, scopes).resolve_call_target(node, parent)
            && let Some(ns) = namespace.opt_symbol()
        {
            ns.add_dependency(sym);
        }
    }

    fn visit_block(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let sn = node.as_scope().unwrap();
        scopes.push_scope(sn.scope().id());
        self.visit_children(unit, node, scopes, namespace, parent);
        scopes.pop_scope();
    }

    fn visit_type_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(type_ident) = node.child_identifier_by_field(*unit, LangRust::field_name)
            && let Some(type_sym) = type_ident.opt_symbol()
            && let Some(type_node) = node.child_by_field(*unit, LangRust::field_type)
            && let Some(ty) = ExprResolver::new(unit, scopes).infer_type_from_expr(&type_node)
        {
            type_sym.set_type_of(ty.id());
            type_sym.add_dependency(ty);
        }
    }

    fn visit_type_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(type_ident) = node.child_identifier_by_field(*unit, LangRust::field_name)
            && let Some(type_sym) = type_ident.opt_symbol()
            && let Some(default_type_node) =
                node.child_by_field(*unit, LangRust::field_default_type)
            && let Some(ty) =
                ExprResolver::new(unit, scopes).infer_type_from_expr(&default_type_node)
        {
            type_sym.add_dependency(ty);
        }
    }

    fn visit_associated_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(type_ident) = node.child_identifier_by_field(*unit, LangRust::field_name)
            && let Some(type_sym) = type_ident.opt_symbol()
            && let Some(type_node) = node.child_by_field(*unit, LangRust::field_default_type)
            && let Some(ty) = ExprResolver::new(unit, scopes).infer_type_from_expr(&type_node)
        {
            type_sym.add_dependency(ty);
        }
    }

    fn visit_field_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(name_node) = node.child_identifier_by_field(*unit, LangRust::field_name)
            && let Some(symbol) = name_node.opt_symbol()
            && let Some(type_node) = node.child_by_field(*unit, LangRust::field_type)
            && let Some(ty) = ExprResolver::new(unit, scopes).infer_type_from_expr(&type_node)
        {
            symbol.set_type_of(ty.id());
            symbol.add_dependency(ty);
        }
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

        if let Some(name_node) = node.child_identifier_by_field(*unit, LangRust::field_name)
            && let Some(symbol) = name_node.opt_symbol()
            && let Some(value_node) = node.child_by_field(*unit, LangRust::field_value)
            && let Some(ty) = ExprResolver::new(unit, scopes).infer_type_from_expr(&value_node)
        {
            symbol.add_dependency(ty);
        }
    }

    fn visit_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(pattern) = node.child_by_field(*unit, LangRust::field_pattern)
            && let Some(ident) = pattern.find_identifier(*unit)
            && let Some(symbol) = ident.opt_symbol()
            && let Some(type_node) = node.child_by_field(*unit, LangRust::field_type)
            && let Some(ty) = ExprResolver::new(unit, scopes).infer_type_from_expr(&type_node)
        {
            symbol.set_type_of(ty.id());
            symbol.add_dependency(ty);
        }
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
        self.visit_children(unit, node, scopes, namespace, parent);

        let (ty, type_args) =
            if let Some(type_node) = node.child_by_field(*unit, LangRust::field_type) {
                let mut resolver = ExprResolver::new(unit, scopes);
                let ty = resolver.infer_type_from_expr(&type_node);
                let type_args = resolver.collect_type_argument_symbols(&type_node);
                (ty, type_args)
            } else if let Some(value_node) = node.child_by_field(*unit, LangRust::field_value) {
                let ty = ExprResolver::new(unit, scopes).infer_type_from_expr(&value_node);
                (ty, Vec::new())
            } else {
                (None, Vec::new())
            };

        if let Some(ty) = ty {
            if let Some(pattern) = node.child_by_field(*unit, LangRust::field_pattern) {
                Self::bind_pattern_to_type(unit, &pattern, ty);
            }

            if let Some(ns) = namespace.opt_symbol() {
                ns.add_dependency(ty);
                // Also add dependencies on type arguments to the parent
                for arg_sym in &type_args {
                    ns.add_dependency(arg_sym);
                }
            }
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
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(name_node) = node
            .child_by_field(*unit, LangRust::field_name)
            .or_else(|| node.child_by_field(*unit, LangRust::field_type))
            && let Some(ty) = ExprResolver::new(unit, scopes).infer_type_from_expr(&name_node)
            && let Some(caller) = parent
        {
            caller.add_dependency(ty);
        }
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

    #[test]
    fn test_const_item() {
        let source = r#"
fn run() {
    const X: i32 = 42;
}
"#;
        assert_dependencies(
            &[source],
            &[("run", SymKind::Function, &["_c::_m::source_0::run::X"])],
        );
    }

    #[test]
    fn test_call_expression_simple() {
        let source = r#"
fn foo() {}
fn bar() {
    foo();
}
"#;
        assert_dependencies(
            &[source],
            &[("bar", SymKind::Function, &["_c::_m::source_0::foo"])],
        );
    }

    #[test]
    fn test_call_expression_method() {
        let source = r#"
struct S;
impl S {
    fn method(&self) {}
}
fn run() {
    let s = S;
    s.method();
}
"#;
        assert_dependencies(
            &[source],
            &[(
                "run",
                SymKind::Function,
                &["_c::_m::source_0::S", "_c::_m::source_0::S::method"],
            )],
        );
    }

    #[test]
    fn test_call_expression_associated() {
        let source = r#"
struct S;
impl S {
    fn new() -> S { S }
}
fn run() {
    S::new();
}
"#;
        assert_dependencies(
            &[source],
            &[(
                "run",
                SymKind::Function,
                &["_c::_m::source_0::S", "_c::_m::source_0::S::new"],
            )],
        );
    }

    #[test]
    fn test_call_expression_nested() {
        let source = r#"
fn a() -> i32 { 0 }
fn b(_x: i32) {}
fn run() {
    b(a());
}
"#;
        assert_dependencies(
            &[source],
            &[(
                "run",
                SymKind::Function,
                &["_c::_m::source_0::a", "_c::_m::source_0::b"],
            )],
        );
    }

    #[test]
    fn test_call_expression_chain() {
        let source = r#"
struct S;
impl S {
    fn foo(&self) -> S { S }
    fn bar(&self) {}
}
fn run() {
    let s = S;
    s.foo().bar();
}
"#;
        assert_dependencies(
            &[source],
            &[(
                "run",
                SymKind::Function,
                &[
                    "_c::_m::source_0::S",
                    "_c::_m::source_0::S::foo",
                    "_c::_m::source_0::S::bar",
                ],
            )],
        );
    }

    #[test]
    fn test_let_declaration_inference() {
        let source = r#"
fn run() {
    let x = 42;
    let y: f64 = 3.14;
}
"#;
        assert_symbol_type(&[source], "x", SymKind::Variable, Some("i32"));
        assert_symbol_type(&[source], "y", SymKind::Variable, Some("f64"));
    }

    #[test]
    fn test_let_declaration_struct_pattern() {
        let source = r#"
    struct Point { x: i32, y: i32 }
    fn run() {
        let p = Point { x: 1, y: 2 };
        let Point { x, y } = p;
    }
    "#;
        // Test that struct pattern destructuring correctly infers field types
        assert_symbol_type(&[source], "p", SymKind::Variable, Some("Point"));
        assert_symbol_type(&[source], "x", SymKind::Variable, Some("i32"));
        assert_symbol_type(&[source], "y", SymKind::Variable, Some("i32"));
    }

    #[test]
    fn test_let_declaration_struct_pattern_with_alias() {
        let source = r#"
    struct Point { x: i32, y: i32 }
    fn run() {
        let Point { x: px, y: py } = Point { x: 1, y: 2 };
    }
    "#;
        assert_symbol_type(&[source], "px", SymKind::Variable, Some("i32"));
        assert_symbol_type(&[source], "py", SymKind::Variable, Some("i32"));
    }
}
