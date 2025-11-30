use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirId, HirIdent, HirKind, HirNode, HirScope};
use llmcc_core::next_hir_id;
use llmcc_core::scope::{Scope, ScopeStack};
use llmcc_core::symbol::{ScopeId, SymKind, Symbol};
use llmcc_resolver::{CollectorScopes, ResolverOption};

use std::collections::HashMap;

use crate::LangRust;
use crate::token::AstVisitorRust;
use crate::util::{parse_crate_name, parse_file_name, parse_module_name};

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

    /// Declare a symbol from a named field in the AST node
    fn declare_symbol(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &CollectorScopes<'tcx>,
        kind: SymKind,
        field_id: u16,
    ) -> Option<&'tcx Symbol> {
        let ident = node.child_identifier_by_field(*unit, field_id)?;
        tracing::trace!("declaring symbol '{}' of kind {:?}", ident.name, kind);
        let sym = scopes.lookup_or_insert(&ident.name, node, kind)?;
        ident.set_symbol(sym);
        Some(sym)
    }

    /// Find all identifiers in a pattern node (recursive)
    fn collect_pattern_identifiers(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &CollectorScopes<'tcx>,
        kind: SymKind,
    ) -> Vec<&'tcx Symbol> {
        let mut symbols = Vec::new();
        Self::collect_pattern_identifiers_impl(unit, node, scopes, kind, &mut symbols);
        symbols
    }

    /// Recursive worker for [`collect_pattern_identifiers`].
    ///
    /// Examples of the shapes we cover:
    /// - `let (a, b): (i32, i32)` will visit both tuple elements.
    /// - `let Foo { x, y: (left, right) } = value;` walks nested struct/tuple patterns.
    fn collect_pattern_identifiers_impl(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &CollectorScopes<'tcx>,
        kind: SymKind,
        symbols: &mut Vec<&'tcx Symbol>,
    ) {
        if matches!(
            node.kind_id(),
            // Skip non-binding identifiers
            LangRust::type_identifier | LangRust::primitive_type | LangRust::field_identifier |
            // Special handling for scoped identifiers: don't collect them as variables
            LangRust::scoped_identifier | LangRust::scoped_type_identifier
        ) {
            return;
        }

        if let Some(ident) = node.as_ident() {
            let name = ident.name.to_string();
            let sym = scopes.lookup_or_insert(&name, node, kind);

            if let Some(sym) = sym {
                ident.set_symbol(sym);
                symbols.push(sym);
            }
        }
        for child in node.children(unit) {
            Self::collect_pattern_identifiers_impl(unit, &child, scopes, kind, symbols);
        }
    }

    fn alloc_scope(&mut self, unit: &CompileUnit<'tcx>, symbol: &'tcx Symbol) -> &'tcx Scope<'tcx> {
        let scope = unit.cc.alloc_scope(symbol.owner());
        scope.set_symbol(symbol);
        self.scope_map.insert(scope.id(), scope);
        scope
    }

    fn get_scope(&self, scope_id: ScopeId) -> &'tcx Scope<'tcx> {
        self.scope_map.get(&scope_id).copied().unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    fn visit_with_scope(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        sym: &'tcx Symbol,
        sn: &'tcx HirScope<'tcx>,
        ident: &'tcx HirIdent<'tcx>,
    ) {
        tracing::trace!(
            "visiting with scope for symbol {}",
            sym.format(Some(scopes.interner()))
        );
        ident.set_symbol(sym);
        sn.set_ident(ident);

        let scope = if sym.opt_scope().is_none() {
            tracing::trace!(
                "allocating new scope for symbol {}",
                sym.format(Some(scopes.interner()))
            );
            self.alloc_scope(unit, sym)
        } else {
            tracing::trace!(
                "use existing scope for symbol {}",
                sym.format(Some(scopes.interner()))
            );
            self.get_scope(sym.scope())
        };
        sym.set_scope(scope.id());
        sn.set_scope(scope);

        scopes.push_scope(scope);
        self.visit_children(unit, node, scopes, scope, Some(sym));
        scopes.pop_scope();
    }

    #[allow(clippy::too_many_arguments)]
    fn visit_scoped_named(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
        kind: SymKind,
        field_id: u16,
    ) {
        tracing::trace!("visiting scoped named node with kind {:?}", kind);
        if let Some((sn, ident)) = node.scope_and_ident_by_field(*unit, field_id) {
            // first try normal lookup
            if let Some(sym) = scopes.lookup_symbol(&ident.name, vec![kind]) {
                tracing::trace!("found existing symbol '{}' of kind {:?}", ident.name, kind);
                self.visit_with_scope(unit, node, scopes, sym, sn, ident);
            }
            // try unresolved type if not found
            else if let Some(sym) =
                scopes.lookup_symbol(&ident.name, vec![SymKind::UnresolvedType])
            {
                tracing::trace!("found existing unresolved symbol '{}'", ident.name);
                sym.set_kind(kind);
                self.visit_with_scope(unit, node, scopes, sym, sn, ident);
            }
            // finally insert new symbol
            else if let Some(sym) = scopes.lookup_or_insert(&ident.name, node, kind) {
                tracing::trace!("inserting new symbol '{}' of kind {:?}", ident.name, kind);
                self.visit_with_scope(unit, node, scopes, sym, sn, ident);
            }
        } else {
            tracing::warn!("scoped named node is missing scope or ident, skipping");
        }
    }
}

impl<'tcx> AstVisitorRust<'tcx, CollectorScopes<'tcx>> for CollectorVisitor<'tcx> {
    fn visit_block(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            tracing::trace!("visiting block node {}", node.format(*unit));
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);

            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        }
    }

    #[rustfmt::skip]
    fn visit_source_file(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let file_path = unit.file_path().expect("no file path found to compile");
        let start_depth = scopes.scope_depth();

        // Parse crate name and set up crate scope
        if let Some(crate_name) = parse_crate_name(file_path)
            && let Some(symbol) = scopes.lookup_or_insert_global(&crate_name, node, SymKind::Crate)
        {
            tracing::trace!("insert crate symbol in globals '{}'", crate_name);
            scopes.push_scope_with(node, Some(symbol));
        }

        if let Some(module_name) = parse_module_name(file_path)
            && let Some(symbol) = scopes.lookup_or_insert_global(&module_name, node, SymKind::Module)
        {
            tracing::trace!("insert module symbol in globals '{}'", module_name);
            scopes.push_scope_with(node, Some(symbol));
        }

        let sn = node.as_scope().unwrap();
        if let Some(file_name) = parse_file_name(file_path)
            && let Some(file_sym) = scopes.lookup_or_insert_global(&file_name, node, SymKind::File)
        {
            tracing::trace!("insert file symbol in globals '{}'", file_name);
            let ident = unit.cc.alloc_file_ident(next_hir_id(), &file_name, file_sym);
            ident.set_symbol(file_sym);
            sn.set_ident(ident);

            let scope = self.alloc_scope(unit, file_sym);
            file_sym.set_scope(scope.id());
            sn.set_scope(scope);

            scopes.push_scope(scope);

            if let Some(crate_sym) = scopes.lookup_or_insert_global("crate", node, SymKind::Module) {
                tracing::trace!("insert 'crate' symbol in globals");
                crate_sym.set_scope(scopes.globals().id());
            }
        }

        for child in node.children(unit) {
            self.visit_node(unit, &child, scopes, namespace, parent);
        }

        scopes.pop_until(start_depth);
    }

    fn visit_mod_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if node.child_by_field(*unit, LangRust::field_body).is_none() {
            return;
        }
        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            SymKind::Namespace,
            LangRust::field_name,
        );
    }

    fn visit_function_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            SymKind::Function,
            LangRust::field_name,
        );
    }

    fn visit_function_signature_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            SymKind::Function,
            LangRust::field_name,
        );
    }

    fn visit_struct_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            SymKind::Struct,
            LangRust::field_name,
        );
    }
    fn visit_enum_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            SymKind::Enum,
            LangRust::field_name,
        );
    }

    fn visit_trait_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            SymKind::Trait,
            LangRust::field_name,
        );
    }

    fn visit_impl_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        // if let Some(trait_node) = node.child_by_field(*unit, LangRust::field_trait)
        //     && let Some(trait_name) = Self::type_name_from_node(unit, &trait_node)
        // {
        //     if let Some(symbol) =
        //         scopes.lookup_symbol(trait_name, Some(vec![SymKind::Trait]), None)
        //         && let Some(trait_ident) =
        //             node.child_identifier_by_field(*unit, LangRust::field_trait)
        //     {
        //         trait_ident.set_symbol(symbol);
        //     } else if let Some(symbol) =
        //         scopes.lookup_or_insert(trait_name, node, SymKind::UnresolvedType)
        //         && let Some(trait_ident) =
        //             node.child_identifier_by_field(*unit, LangRust::field_trait)
        //     {
        //         trait_ident.set_symbol(symbol);
        //     }
        // }

        // let Some(sn) = node.as_scope() else {
        //     return;
        // };

        // if let Some(type_ident) = node.child_identifier_by_field(*unit, LangRust::field_type)
        //     && let Some(type_node) = node.child_by_field(*unit, LangRust::field_type)
        //     && let Some(type_name) = Self::type_name_from_node(unit, &type_node)
        // {
        //     if let Some(symbol) = scopes.lookup_symbol(
        //         type_name,
        //         Some(vec![SymKind::Struct, SymKind::Enum, SymKind::Primitive]),
        //         None,
        //     ) {
        //         type_ident.set_symbol(symbol);
        //         // Primitives don't have scopes, so we need to allocate one for the impl
        //         let needs_scope = symbol.opt_scope().is_none();
        //         self.visit_with_scope(unit, node, scopes, symbol, sn, type_ident, needs_scope);
        //         return;
        //     } else if let Some(symbol) =
        //         scopes.lookup_or_insert(type_name, node, SymKind::UnresolvedType)
        //     {
        //         type_ident.set_symbol(symbol);
        //         self.visit_with_scope(unit, node, scopes, symbol, sn, type_ident, true);
        //         return;
        //     }
        // }
    }

    fn visit_macro_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            SymKind::Macro,
            LangRust::field_name,
        );
    }

    fn visit_const_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(symbol) =
            self.declare_symbol(unit, node, scopes, SymKind::Const, LangRust::field_name)
        {
            self.visit_children(unit, node, scopes, namespace, Some(symbol));
        }
    }

    fn visit_static_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(symbol) =
            self.declare_symbol(unit, node, scopes, SymKind::Static, LangRust::field_name)
        {
            self.visit_children(unit, node, scopes, namespace, Some(symbol));
        }
    }

    fn visit_type_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(symbol) =
            self.declare_symbol(unit, node, scopes, SymKind::TypeAlias, LangRust::field_name)
        {
            self.visit_children(unit, node, scopes, namespace, Some(symbol));
        }
    }

    fn visit_type_identifier(
        &mut self,
        _unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let ident = node.as_ident().unwrap();
        if let Some(symbol) = scopes.lookup_symbol(
            &ident.name,
            vec![
                SymKind::Struct,
                SymKind::Enum,
                SymKind::Trait,
                SymKind::Function,
                SymKind::TypeAlias,
            ],
        ) {
            ident.set_symbol(symbol);
            return;
        }
    }

    fn visit_type_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.declare_symbol(
            unit,
            node,
            scopes,
            SymKind::TypeParameter,
            LangRust::field_name,
        );
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_const_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sym) =
            self.declare_symbol(unit, node, scopes, SymKind::Const, LangRust::field_name)
            && let Some(owner) = namespace.opt_symbol()
        {
            owner.add_dependency(sym, None);
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_associated_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.declare_symbol(unit, node, scopes, SymKind::TypeAlias, LangRust::field_name);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_where_predicate(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let _ = self.declare_symbol(unit, node, scopes, SymKind::Field, LangRust::field_name);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_array_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_tuple_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_primitive_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_abstract_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
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
        let _ = self.declare_symbol(unit, node, scopes, SymKind::Field, LangRust::field_name);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_enum_variant(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Get the parent enum symbol before creating the variant
        let parent_enum = parent.or_else(|| namespace.opt_symbol());

        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            SymKind::EnumVariant,
            LangRust::field_name,
        );

        // Set type_of on the variant to point to the parent enum
        if let Some(enum_sym) = parent_enum
            && let Some(ident) = node.child_identifier_by_field(*unit, LangRust::field_name)
            && let Some(variant_sym) = scopes.lookup_symbol(&ident.name, vec![SymKind::EnumVariant])
        {
            variant_sym.set_type_of(enum_sym.id);
        }
    }

    fn visit_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(symbol) = self.declare_symbol(
            unit,
            node,
            scopes,
            SymKind::Variable,
            LangRust::field_pattern,
        ) {
            self.visit_children(unit, node, scopes, namespace, Some(symbol));
        }
    }

    fn visit_closure_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Create a scope for the closure
        if let Some(sn) = node.as_scope() {
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);

            // Link scope to parent namespace
            scope.add_parent(namespace);

            scopes.push_scope(scope);

            // Collect closure parameters
            if let Some(params) = node.child_by_field(*unit, LangRust::field_parameters) {
                let _ = Self::collect_pattern_identifiers(unit, &params, scopes, SymKind::Variable);
            }

            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        }
    }

    fn visit_let_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Check if value is a closure expression to determine symbol kind
        let is_closure = node
            .child_by_field(*unit, LangRust::field_value)
            .map(|v| v.kind_id() == LangRust::closure_expression)
            .unwrap_or(false);

        let kind = if is_closure {
            SymKind::Closure
        } else {
            SymKind::Variable
        };

        // Collect the pattern identifier(s) with appropriate kind
        let let_syms = if let Some(pattern) = node.child_by_field(*unit, LangRust::field_pattern) {
            Self::collect_pattern_identifiers(unit, &pattern, scopes, kind)
        } else {
            vec![]
        };

        // For closures, pass the let symbol as parent so closure scope gets linked
        // Use first symbol if it's a simple pattern, otherwise use parent
        if is_closure && !let_syms.is_empty() {
            self.visit_children(unit, node, scopes, namespace, Some(let_syms[0]));
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
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
    let unit_globals = arena.alloc(Scope::new(HirId(unit.index)));
    let mut scopes = CollectorScopes::new(cc, unit.index, scope_stack, unit_globals);

    let mut visit = CollectorVisitor::new();
    visit.visit_node(&unit, node, &mut scopes, unit_globals, None);

    unit_globals
}

#[cfg(test)]
mod tests {
    use crate::token::LangRust;

    use llmcc_core::context::CompileCtxt;
    use llmcc_core::ir_builder::{IrBuildOption, build_llmcc_ir};
    use llmcc_core::symbol::{SymId, SymKind};
    use llmcc_resolver::{ResolverOption, bind_symbols_with, collect_symbols_with};
    fn with_compiled_unit<F>(sources: &[&str], check: F)
    where
        F: for<'a> FnOnce(&'a CompileCtxt<'a>),
    {
        let bytes = sources
            .iter()
            .map(|src| src.as_bytes().to_vec())
            .collect::<Vec<_>>();
        let cc = CompileCtxt::from_sources::<LangRust>(&bytes);
        build_llmcc_ir::<LangRust>(&cc, IrBuildOption::default()).unwrap();
        let resolver_option = ResolverOption::default()
            .with_sequential(true)
            .with_print_ir(true);
        let globals = collect_symbols_with::<LangRust>(&cc, &resolver_option);
        bind_symbols_with::<LangRust>(&cc, globals, &resolver_option);
        check(&cc);
    }

    fn find_symbol_id<'a>(
        cc: &'a CompileCtxt<'a>,
        name: &str,
        kind: SymKind,
    ) -> llmcc_core::symbol::SymId {
        let name_key = cc.interner.intern(name);
        cc.get_all_symbols()
            .into_iter()
            .find(|symbol| symbol.name == name_key && symbol.kind() == kind)
            .map(|symbol| symbol.id())
            .unwrap_or_else(|| panic!("symbol {name} with kind {:?} not found", kind))
    }

    fn dependency_names<'a>(cc: &'a CompileCtxt<'a>, sym_id: SymId) -> Vec<String> {
        let symbol = cc
            .opt_get_symbol(sym_id)
            .unwrap_or_else(|| panic!("missing symbol for id {:?}", sym_id));
        let deps = symbol.depends_ids();
        let mut names = Vec::new();
        for dep in deps {
            if let Some(target) = cc.opt_get_symbol(dep) {
                if let Some(name) = cc.interner.resolve_owned(target.name) {
                    names.push(name);
                }
            }
        }
        names.sort();
        names
    }

    fn type_name_of<'a>(cc: &'a CompileCtxt<'a>, sym_id: SymId) -> Option<String> {
        let symbol = cc.opt_get_symbol(sym_id)?;
        let ty_id = symbol.type_of()?;
        let ty_symbol = cc.opt_get_symbol(ty_id)?;
        cc.interner.resolve_owned(ty_symbol.name)
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
                    "dependency mismatch for symbol {name}: expected suffixes {:?}, actual dependencies {:?}, missing {:?}",
                    expected,
                    actual,
                    missing
                );
            }
        });
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

    #[serial_test::serial]
    #[test]
    fn call_expression_basic_dependency() {
        let source = r#"
fn callee() {}
fn caller() {
    callee();
}
"#;
        assert_dependencies(&[source], &[("caller", SymKind::Function, &["callee"])]);
    }

    #[serial_test::serial]
    #[test]
    fn method_call_dependency_expr() {
        let source = r#"
struct MyStruct;
impl MyStruct {
    fn foo(&self) {}
}

fn run() {
    let s = MyStruct;
    s.foo();
}
"#;
        assert_dependencies(
            &[source],
            &[("run", SymKind::Function, &["MyStruct", "foo"])],
        );
    }

    #[serial_test::serial]
    #[test]
    fn method_call_dependency_chained() {
        let source = r#"
struct Response;

struct RequestBuilder;

impl RequestBuilder {
    fn new() -> Self { RequestBuilder }
    fn set_header(self, _: &str) -> Self { self }
    fn send(self) -> Response { Response }
}

fn execute() -> Response {
    RequestBuilder::new().set_header("x-header").send()
}
"#;
        // Scoped function calls should only depend on the method, not the struct
        // Response is still a dependency because it's the return type
        assert_dependencies(
            &[source],
            &[(
                "execute",
                SymKind::Function,
                &["new", "set_header", "send", "Response"],
            )],
        );
    }

    #[serial_test::serial]
    #[test]
    fn wrapped_call_dependency() {
        let source = r#"
async fn async_task() {}
fn maybe() -> Result<(), ()> { Ok(()) }

async fn entry() -> Result<(), ()> {
    (async_task)().await;
    (maybe)()?;
    Ok(())
}
"#;
        assert_dependencies(
            &[source],
            &[("entry", SymKind::Function, &["async_task", "maybe"])],
        );
    }

    #[serial_test::serial]
    #[test]
    fn macro_invocation_dependency() {
        let source = r#"
macro_rules! ping { () => {} }

fn call_macro() {
    ping!();
}
"#;
        assert_dependencies(&[source], &[("call_macro", SymKind::Function, &["ping"])]);
    }

    #[serial_test::serial]
    #[test]
    fn scoped_function_dependency() {
        let source = r#"
mod helpers {
    pub fn compute() {}
}

fn run() {
    helpers::compute();
}
"#;
        assert_dependencies(&[source], &[("run", SymKind::Function, &["compute"])]);
    }

    #[serial_test::serial]
    #[test]
    fn associated_function_dependency() {
        let source = r#"
struct Foo;
impl Foo {
    fn build() -> Self {
        Foo
    }
}

fn run() {
    Foo::build();
}
"#;
        // Scoped function calls should only depend on the method, not the struct
        assert_dependencies(&[source], &[("run", SymKind::Function, &["build"])]);
    }

    #[serial_test::serial]
    #[test]
    fn trait_fully_qualified_call_dependency() {
        let source = r#"
trait Greeter {
    fn greet();
}

struct Foo;

impl Greeter for Foo {
    fn greet() {}
}

fn run() {
    <Foo as Greeter>::greet();
}
"#;
        // Scoped function calls should only depend on the method, not the struct
        assert_dependencies(&[source], &[("run", SymKind::Function, &["greet"])]);
    }

    #[serial_test::serial]
    #[test]
    fn closure_symbol_kind() {
        let source = r#"
fn caller() {
    let add_one = |n: i32| n + 1;
    let mul = |a, b| a * b;
    let product = mul(3, 4);
    let closure_with_block = |x| {
        let y = x + 1;
        y * 2
    };
}
"#;
        with_compiled_unit(&[source], |cc| {
            // Check that closures are found as Closure kind
            let add_one_sym = find_symbol_id(cc, "add_one", SymKind::Closure);
            assert!(add_one_sym.0 > 0, "add_one should be found as Closure");

            let mul_sym = find_symbol_id(cc, "mul", SymKind::Closure);
            assert!(mul_sym.0 > 0, "mul should be found as Closure");

            let closure_with_block_sym = find_symbol_id(cc, "closure_with_block", SymKind::Closure);
            assert!(
                closure_with_block_sym.0 > 0,
                "closure_with_block should be found as Closure"
            );
        });
    }

    #[serial_test::serial]
    #[test]
    fn namespaced_macro_dependency() {
        let source = r#"
mod outer {
    mod inner {
        fn shout() {
        }
    }
}

fn run() {
    outer::inner::shout!();
}
"#;
        assert_dependencies(&[source], &[("run", SymKind::Function, &["shout"])]);
    }

    #[serial_test::serial]
    #[test]
    fn super_module_function_dependency() {
        let source = r#"
mod outer {
    pub fn top() {}
    pub mod inner {
        pub fn run() {
            super::top();
        }
    }
}
"#;
        assert_dependencies(&[source], &[("run", SymKind::Function, &["top"])]);
    }

    #[serial_test::serial]
    #[test]
    fn variable_type_annotation() {
        let source = r#"
struct Foo;

fn run() {
    let value: Foo = Foo;
    let other = Foo;
}
"#;
        assert_symbol_type(&[source], "value", SymKind::Variable, Some("Foo"));
        assert_symbol_type(&[source], "other", SymKind::Variable, Some("Foo"));
    }

    #[serial_test::serial]
    #[test]
    fn static_type_annotation() {
        let source = r#"
struct Foo;
static GLOBAL: Foo = Foo;
"#;
        assert_symbol_type(&[source], "GLOBAL", SymKind::Static, Some("Foo"));
    }

    #[serial_test::serial]
    #[test]
    fn parameter_type_annotation() {
        let source = r#"
struct Foo;

fn consume(param: Foo) {
    let _ = param;
}
"#;
        assert_symbol_type(&[source], "param", SymKind::Variable, Some("Foo"));
    }

    #[serial_test::serial]
    #[test]
    fn field_type_annotation() {
        let source = r#"
struct Bar;
struct Bucket {
    item: Bar,
}
"#;
        assert_symbol_type(&[source], "item", SymKind::Field, Some("Bar"));
    }

    #[serial_test::serial]
    #[test]
    fn const_and_type_alias_types() {
        let source = r#"
struct Foo;
type Alias = Foo;
const ANSWER: i32 = 42;
"#;
        assert_symbol_type(&[source], "Alias", SymKind::TypeAlias, Some("Foo"));
        assert_symbol_type(&[source], "ANSWER", SymKind::Const, Some("i32"));
    }

    #[serial_test::serial]
    #[test]
    fn struct_field_generic_dependency() {
        let source = r#"
struct Foo;
struct List<T>(T);

struct Container {
    data: List<Foo>,
}
"#;
        assert_dependencies(
            &[source],
            &[
                ("Container", SymKind::Struct, &["Foo", "List"]),
                ("data", SymKind::Field, &["Foo", "List"]),
            ],
        );
    }

    #[serial_test::serial]
    #[test]
    fn enum_variant_dependency() {
        let source = r#"
struct Foo;
enum Wrapper {
    Item(Foo),
}
"#;
        assert_dependencies(&[source], &[("Wrapper", SymKind::Enum, &["Foo"])]);
    }

    #[serial_test::serial]
    #[test]
    fn let_statement_generic_dependency() {
        let source = r#"
struct Foo;
struct Bar;
enum Result<T, E> {
    Ok(T),
    Err(E),
}

fn run() {
    let value: Result<Foo, Bar> = Result::Ok(Foo);
}
"#;
        assert_dependencies(
            &[source],
            &[
                ("value", SymKind::Variable, &["Bar", "Foo", "Result"]),
                ("run", SymKind::Function, &["Bar", "Foo", "Result"]),
            ],
        );
    }
}
