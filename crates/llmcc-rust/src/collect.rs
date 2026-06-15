use llmcc_core::ResolveOptions;
use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirId, HirIdent, HirNode, HirScope};
use llmcc_core::next_hir_id;
use llmcc_core::scope::{Scope, ScopeStack};
use llmcc_core::symbol::{ScopeId, SymKind, SymKindSet, Symbol};
use llmcc_resolver::CollectCtxt;

use std::collections::HashMap;

use crate::LangRust;
use crate::token::AstVisitorRust;

/// True when a callable declaration should be owned by a type-like scope.
fn is_method_context(parent: Option<&Symbol>) -> bool {
    parent.is_some_and(|p| {
        matches!(
            p.kind(),
            SymKind::Struct | SymKind::Enum | SymKind::Trait | SymKind::UnresolvedType
        )
    })
}

/// Add a symbol to global lookup without changing its exported/global flag.
fn index_global<'tcx>(ctxt: &CollectCtxt<'tcx>, symbol: &'tcx Symbol) {
    let mut exists = false;
    ctxt.globals().for_each_symbol(|existing| {
        if existing.id() == symbol.id() {
            exists = true;
        }
    });
    if !exists {
        ctxt.globals().insert(symbol);
    }
}

fn publish_global<'tcx>(ctxt: &CollectCtxt<'tcx>, symbol: &'tcx Symbol) {
    symbol.set_is_global(true);
    index_global(ctxt, symbol);
}

/// Runs immediately after a symbol-owned scope is pushed.
type ScopeHook<'tcx> = Box<dyn FnOnce(&HirNode<'tcx>, &mut CollectCtxt<'tcx>) + 'tcx>;

struct ScopedSymbol<'tcx> {
    symbol: &'tcx Symbol,
    scope_node: &'tcx HirScope<'tcx>,
    ident: &'tcx HirIdent<'tcx>,
}

#[derive(Debug)]
struct CollectorVisitor<'tcx> {
    scope_map: HashMap<ScopeId, &'tcx Scope<'tcx>>,
}

impl<'tcx> CollectorVisitor<'tcx> {
    fn new() -> Self {
        Self {
            scope_map: HashMap::new(),
        }
    }

    /// Declare the identifier at `field_id`, or the scope identifier when absent.
    fn declare_symbol(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &CollectCtxt<'tcx>,
        kind: SymKind,
        field_id: u16,
    ) -> Option<&'tcx Symbol> {
        let ident = node
            .query(unit)
            .try_ident_with_field(field_id)
            .or_else(|| node.as_scope().and_then(|sn| sn.try_ident()))?;

        let sym = ctxt.declare(ident.name, node, kind)?;
        ident.set_symbol(sym);

        if let Some(sn) = node.as_scope() {
            sn.set_ident(ident);
        }

        Some(sym)
    }

    /// Declare every binding identifier in a Rust pattern.
    fn collect_pattern_identifiers(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &CollectCtxt<'tcx>,
        kind: SymKind,
    ) -> Vec<&'tcx Symbol> {
        let mut symbols = Vec::new();
        Self::collect_pattern_identifiers_impl(unit, node, ctxt, kind, &mut symbols);
        symbols
    }

    /// Walk nested pattern forms and declare only binding identifiers.
    fn collect_pattern_identifiers_impl(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &CollectCtxt<'tcx>,
        kind: SymKind,
        symbols: &mut Vec<&'tcx Symbol>,
    ) {
        if matches!(
            node.kind_id(),
            // Type/path identifiers are references, not local bindings.
            LangRust::type_identifier
                | LangRust::primitive_type
                | LangRust::field_identifier
                | LangRust::scoped_identifier
                | LangRust::scoped_type_identifier
        ) {
            return;
        }

        if let Some(ident) = node.as_ident() {
            let name = ident.name.to_string();
            let sym = ctxt.declare(&name, node, kind);

            if let Some(sym) = sym {
                ident.set_symbol(sym);
                symbols.push(sym);
            }
        }
        for &child_id in node.child_ids() {
            let child = unit.hir_node(child_id);
            Self::collect_pattern_identifiers_impl(unit, &child, ctxt, kind, symbols);
        }
    }

    fn alloc_symbol_scope(
        &mut self,
        unit: &CompileUnit<'tcx>,
        symbol: &'tcx Symbol,
    ) -> &'tcx Scope<'tcx> {
        let scope = unit.context().alloc_scope(symbol.owner());
        scope.set_symbol(symbol);
        self.scope_map.insert(scope.id(), scope);
        scope
    }

    fn cached_scope(&self, scope_id: ScopeId) -> Option<&'tcx Scope<'tcx>> {
        self.scope_map.get(&scope_id).copied()
    }

    /// Reuse a symbol of `kind`, upgrade an unresolved placeholder, or declare one.
    fn declare_or_upgrade(
        &mut self,
        unit: &CompileUnit<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        name: &str,
        node: &HirNode<'tcx>,
        kind: SymKind,
    ) -> Option<&'tcx Symbol> {
        if let Some(symbol) = ctxt.lookup_symbol(name, SymKindSet::from_kind(kind)) {
            return Some(symbol);
        }

        if let Some(symbol) =
            ctxt.lookup_symbol(name, SymKindSet::from_kind(SymKind::UnresolvedType))
        {
            symbol.set_kind(kind);
            return Some(symbol);
        }

        if let Some(symbol) = ctxt.declare(name, node, kind) {
            if symbol.try_owned_scope().is_none() {
                let scope = self.alloc_symbol_scope(unit, symbol);
                symbol.set_owned_scope(scope.id());
            }
            return Some(symbol);
        }

        None
    }

    /// Visit a symbol-owned scope and restore the previous collector depth.
    fn visit_symbol_scope(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        scoped: ScopedSymbol<'tcx>,
        on_scope_enter: Option<ScopeHook<'tcx>>,
    ) {
        let ScopedSymbol {
            symbol,
            scope_node,
            ident,
        } = scoped;

        ident.set_symbol(symbol);
        scope_node.set_ident(ident);

        let scope = symbol
            .try_owned_scope()
            .and_then(|scope_id| self.cached_scope(scope_id))
            .unwrap_or_else(|| self.alloc_symbol_scope(unit, symbol));
        symbol.set_owned_scope(scope.id());
        scope_node.set_scope(scope);

        let depth = ctxt.depth();
        ctxt.push_scope(scope);
        if let Some(callback) = on_scope_enter {
            callback(node, ctxt);
        }
        self.visit_children(unit, node, ctxt, scope, Some(symbol));
        ctxt.pop_to(depth);
    }

    /// Declare a named item and visit the scope owned by its symbol.
    fn visit_named_scope(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        kind: SymKind,
        field_id: u16,
        on_scope_enter: Option<ScopeHook<'tcx>>,
    ) {
        if let Some((scope_node, ident)) = node.query(unit).try_scope_and_ident_with_field(field_id)
            && let Some(symbol) = self.declare_or_upgrade(unit, ctxt, ident.name, node, kind)
        {
            if node
                .child_by_kind(unit, LangRust::visibility_modifier)
                .is_some()
            {
                publish_global(ctxt, symbol);
            }
            self.visit_symbol_scope(
                unit,
                node,
                ctxt,
                ScopedSymbol {
                    symbol,
                    scope_node,
                    ident,
                },
                on_scope_enter,
            );
        }
    }
}

impl<'tcx> AstVisitorRust<'tcx, CollectCtxt<'tcx>> for CollectorVisitor<'tcx> {
    /// Blocks create anonymous lexical scopes for local bindings.
    fn visit_block(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            let scope = unit.context().alloc_scope(node.id());
            sn.set_scope(scope);

            ctxt.push_scope(scope);
            self.visit_children(unit, node, ctxt, scope, parent);
            ctxt.pop_scope();
        }
    }

    fn visit_match_block(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_block(unit, node, ctxt, namespace, parent);
    }

    /// Seed package/module/file scopes for one Rust source file.
    fn visit_source_file(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let start_depth = ctxt.depth();
        let meta = unit.unit_meta();

        let crate_scope = meta
            .package_name
            .as_deref()
            .and_then(|name| ctxt.push_package_scope(node, name));

        let module_wrapper_scope = meta
            .module_name
            .as_deref()
            .and_then(|name| ctxt.module_scope(node, name, crate_scope));

        let Some(sn) = node.as_scope() else {
            self.visit_children(unit, node, ctxt, namespace, parent);
            ctxt.pop_to(start_depth);
            return;
        };
        if let Some(ref file_name) = meta.file_name
            && let Some(file_sym) = ctxt.declare(file_name, node, SymKind::File)
        {
            let arena_name = unit.context().arena().alloc_str(file_name);
            let ident = unit
                .context()
                .alloc_file_ident(next_hir_id(), arena_name, file_sym);
            ident.set_symbol(file_sym);
            sn.set_ident(ident);

            let scope = self.alloc_symbol_scope(unit, file_sym);
            file_sym.set_owned_scope(scope.id());
            sn.set_scope(scope);

            // File lookup should see package and selected module namespaces.
            if let Some(crate_s) = crate_scope {
                scope.add_parent(crate_s);
            }
            if let Some(module_s) = module_wrapper_scope {
                scope.add_parent(module_s);
            }

            ctxt.push_scope(scope);

            if let Some(crate_sym) = ctxt.declare_global("crate", node, SymKind::Module) {
                crate_sym.set_owned_scope(ctxt.globals().id());
            }

            // Child files are reachable as modules, but package roots are not.
            if module_wrapper_scope.is_none() && !is_rust_package_root(file_name) {
                ctxt.alias_file_module(node, file_name, scope, crate_scope);
            }
        }

        self.visit_children(unit, node, ctxt, namespace, parent);

        ctxt.pop_to(start_depth);
    }

    /// Inline modules introduce a namespace and a `super` alias to the parent scope.
    fn visit_mod_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        if node.child_by_field(unit, LangRust::field_body).is_none() {
            return;
        }

        let parent_scope_id = ctxt.current().id();

        let on_scope_enter: ScopeHook<'tcx> = Box::new(move |node, ctxt| {
            if let Some(super_sym) = ctxt.declare("super", node, SymKind::Module) {
                super_sym.set_owned_scope(parent_scope_id);
            }
        });

        self.visit_named_scope(
            unit,
            node,
            ctxt,
            SymKind::Namespace,
            LangRust::field_name,
            Some(on_scope_enter),
        );
    }

    /// Declare a function or method and expose free functions for package lookup.
    fn visit_function_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let is_method = is_method_context(parent);
        let kind = if is_method {
            SymKind::Method
        } else {
            SymKind::Function
        };

        self.visit_named_scope(unit, node, ctxt, kind, LangRust::field_name, None);

        // Cross-file and cross-package lookups start from merged unit globals.
        if !is_method
            && let Some((_, ident)) = node
                .query(unit)
                .try_scope_and_ident_with_field(LangRust::field_name)
            && let Some(sym) = ctxt.lookup_symbol(ident.name, SymKindSet::from_kind(kind))
        {
            if ident.name == "main" {
                publish_global(ctxt, sym);
            } else {
                index_global(ctxt, sym);
            }
        }
    }

    /// Function signatures have no body, but still declare callable symbols.
    fn visit_function_signature_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let kind = if is_method_context(parent) {
            SymKind::Method
        } else {
            SymKind::Function
        };
        self.visit_named_scope(unit, node, ctxt, kind, LangRust::field_name, None);
    }

    /// Struct scopes receive `Self` aliases and are indexed for type lookup.
    fn visit_struct_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        self.visit_named_scope(
            unit,
            node,
            ctxt,
            SymKind::Struct,
            LangRust::field_name,
            Some(Box::new(|node, ctxt| {
                let _ = ctxt.declare("self", node, SymKind::TypeAlias);
                let _ = ctxt.declare("Self", node, SymKind::TypeAlias);
            })),
        );

        // Nominal types need package-level lookup before binding runs.
        if let Some((_, ident)) = node
            .query(unit)
            .try_scope_and_ident_with_field(LangRust::field_name)
            && let Some(sym) =
                ctxt.lookup_symbol(ident.name, SymKindSet::from_kind(SymKind::Struct))
        {
            index_global(ctxt, sym);
        }
    }

    /// Enums own variant scopes and are indexed for type lookup.
    fn visit_enum_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        self.visit_named_scope(unit, node, ctxt, SymKind::Enum, LangRust::field_name, None);

        // Nominal types need package-level lookup before binding runs.
        if let Some((_, ident)) = node
            .query(unit)
            .try_scope_and_ident_with_field(LangRust::field_name)
            && let Some(sym) = ctxt.lookup_symbol(ident.name, SymKindSet::from_kind(SymKind::Enum))
        {
            index_global(ctxt, sym);
        }
    }

    /// Traits own associated items and provide `Self` aliases inside the body.
    fn visit_trait_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        self.visit_named_scope(
            unit,
            node,
            ctxt,
            SymKind::Trait,
            LangRust::field_name,
            Some(Box::new(|node, ctxt| {
                let _ = ctxt.declare("self", node, SymKind::TypeAlias);
                let _ = ctxt.declare("Self", node, SymKind::TypeAlias);
            })),
        );

        // Trait references in impls may resolve across files/packages.
        if let Some((_, ident)) = node
            .query(unit)
            .try_scope_and_ident_with_field(LangRust::field_name)
            && let Some(sym) = ctxt.lookup_symbol(ident.name, SymKindSet::from_kind(SymKind::Trait))
        {
            index_global(ctxt, sym);
        }
    }

    /// Impl blocks collect their target placeholder and optional trait reference.
    fn visit_impl_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        if let Some(ti) = node.query(unit).try_ident_with_field(LangRust::field_trait) {
            let symbol = ctxt
                .lookup_symbol(ti.name, SymKindSet::from_kind(SymKind::Trait))
                .or_else(|| {
                    ctxt.lookup_symbol(ti.name, SymKindSet::from_kind(SymKind::UnresolvedType))
                })
                .or_else(|| ctxt.declare(ti.name, node, SymKind::UnresolvedType));
            if let Some(symbol) = symbol {
                ti.set_symbol(symbol);
            }
        }

        if let Some((sn, ti)) = node
            .query(unit)
            .try_scope_and_ident_with_field(LangRust::field_type)
            && let Some(symbol) =
                self.declare_or_upgrade(unit, ctxt, ti.name, node, SymKind::UnresolvedType)
        {
            ti.set_symbol(symbol);
            self.visit_symbol_scope(
                unit,
                node,
                ctxt,
                ScopedSymbol {
                    symbol,
                    scope_node: sn,
                    ident: ti,
                },
                None,
            );
        }
    }

    /// Macro definitions are named callable-like symbols for later references.
    fn visit_macro_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        self.visit_named_scope(unit, node, ctxt, SymKind::Macro, LangRust::field_name, None);
    }

    /// Const declarations behave like named values with optional type dependencies.
    fn visit_const_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        if let Some(symbol) =
            self.declare_symbol(unit, node, ctxt, SymKind::Const, LangRust::field_name)
        {
            self.visit_children(unit, node, ctxt, namespace, Some(symbol));
        }
    }

    /// Statics share const collection behavior but keep their own symbol kind.
    fn visit_static_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        if let Some(symbol) =
            self.declare_symbol(unit, node, ctxt, SymKind::Static, LangRust::field_name)
        {
            self.visit_children(unit, node, ctxt, namespace, Some(symbol));
        }
    }

    /// Type aliases are declared first; binding later attaches the target type.
    fn visit_type_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        if let Some(symbol) =
            self.declare_symbol(unit, node, ctxt, SymKind::TypeAlias, LangRust::field_name)
        {
            self.visit_children(unit, node, ctxt, namespace, Some(symbol));
        }
    }

    /// Generic type parameters are scoped symbols used by later type resolution.
    fn visit_type_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let _ = self.declare_symbol(
            unit,
            node,
            ctxt,
            SymKind::TypeParameter,
            LangRust::field_name,
        );
        self.visit_children(unit, node, ctxt, namespace, parent);
    }

    /// Const generic parameters are value-like symbols in the generic scope.
    fn visit_const_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let _ = self.declare_symbol(unit, node, ctxt, SymKind::Const, LangRust::field_name);
        self.visit_children(unit, node, ctxt, namespace, parent);
    }

    /// Associated types are scoped aliases owned by traits or impls.
    fn visit_associated_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let _ = self.declare_symbol(unit, node, ctxt, SymKind::TypeAlias, LangRust::field_name);
        self.visit_children(unit, node, ctxt, namespace, parent);
    }

    /// Where clauses only contribute referenced bounds today.
    fn visit_where_predicate(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, ctxt, namespace, parent);
    }

    /// Array and tuple type nodes get synthetic composite type symbols.
    fn visit_array_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let _ = self.declare_symbol(
            unit,
            node,
            ctxt,
            SymKind::CompositeType,
            LangRust::field_name,
        );
        self.visit_children(unit, node, ctxt, namespace, parent);
    }

    /// Tuple types reuse the composite-type path used for arrays.
    fn visit_tuple_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_array_type(unit, node, ctxt, namespace, parent);
    }

    /// Primitive types resolve during binding from the initial global scope.
    fn visit_primitive_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, ctxt, namespace, parent);
    }

    /// Abstract types are containers for trait references.
    fn visit_abstract_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, ctxt, namespace, parent);
    }

    /// Aliased imports introduce local type aliases resolved during binding.
    fn visit_use_as_clause(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(alias) = node.query(unit).try_ident_with_field(LangRust::field_alias)
            && let Some(symbol) = ctxt.declare(alias.name, node, SymKind::TypeAlias)
        {
            alias.set_symbol(symbol);
        }

        self.visit_children(unit, node, ctxt, namespace, parent);
    }

    /// Field declarations create member symbols under the current type scope.
    fn visit_field_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let _ = self.declare_symbol(unit, node, ctxt, SymKind::Field, LangRust::field_name);
        self.visit_children(unit, node, ctxt, namespace, parent);
    }

    /// Enum variants remember their owning enum for graph/type relationships.
    fn visit_enum_variant(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let parent_enum = parent.or_else(|| namespace.try_symbol());

        self.visit_named_scope(
            unit,
            node,
            ctxt,
            SymKind::EnumVariant,
            LangRust::field_name,
            None,
        );

        if let Some(enum_sym) = parent_enum
            && let Some(ident) = node.query(unit).try_ident_with_field(LangRust::field_name)
            && let Some(variant_sym) =
                ctxt.lookup_symbol(ident.name, SymKindSet::from_kind(SymKind::EnumVariant))
        {
            variant_sym.set_type_of(enum_sym.id);
        }
    }

    /// Parameters may be simple identifiers or nested destructuring patterns.
    fn visit_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        if let Some(ident) = node
            .query(unit)
            .try_ident_with_field(LangRust::field_pattern)
            && ident.name == "self"
            && let Some(symbol) =
                ctxt.lookup_symbol(ident.name, SymKindSet::from_kind(SymKind::Field))
        {
            ident.set_symbol(symbol);
            self.visit_children(unit, node, ctxt, namespace, Some(symbol));
            return;
        }

        if let Some(pattern) = node.child_by_field(unit, LangRust::field_pattern) {
            if pattern.as_ident().is_some() {
                if let Some(symbol) = self.declare_symbol(
                    unit,
                    node,
                    ctxt,
                    SymKind::Variable,
                    LangRust::field_pattern,
                ) {
                    self.visit_children(unit, node, ctxt, namespace, Some(symbol));
                    return;
                }
            } else {
                let _ = Self::collect_pattern_identifiers(unit, &pattern, ctxt, SymKind::Variable);
                self.visit_children(unit, node, ctxt, namespace, None);
                return;
            }
        }

        if let Some(symbol) =
            self.declare_symbol(unit, node, ctxt, SymKind::Variable, LangRust::field_pattern)
        {
            self.visit_children(unit, node, ctxt, namespace, Some(symbol));
        }
    }

    /// Closures create anonymous scopes and collect parameter bindings up front.
    fn visit_closure_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            let scope = unit.context().alloc_scope(node.id());
            sn.set_scope(scope);

            scope.add_parent(namespace);

            ctxt.push_scope(scope);

            if let Some(params) = node.child_by_field(unit, LangRust::field_parameters) {
                let _ = Self::collect_pattern_identifiers(unit, &params, ctxt, SymKind::Variable);
            }

            self.visit_children(unit, node, ctxt, scope, parent);
            ctxt.pop_scope();
        }
    }

    /// Let patterns declare local bindings; closure values use the binding as owner.
    fn visit_let_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut CollectCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let is_closure = node
            .child_by_field(unit, LangRust::field_value)
            .map(|v| v.kind_id() == LangRust::closure_expression)
            .unwrap_or(false);

        let kind = if is_closure {
            SymKind::Closure
        } else {
            SymKind::Variable
        };

        let let_syms = if let Some(pattern) = node.child_by_field(unit, LangRust::field_pattern) {
            Self::collect_pattern_identifiers(unit, &pattern, ctxt, kind)
        } else {
            vec![]
        };

        // A named closure should own the closure scope when the pattern is simple.
        if is_closure && !let_syms.is_empty() {
            self.visit_children(unit, node, ctxt, namespace, Some(let_syms[0]));
        } else {
            self.visit_children(unit, node, ctxt, namespace, parent);
        }
    }
}

fn is_rust_package_root(file_name: &str) -> bool {
    matches!(file_name, "lib" | "main")
}

pub(crate) fn collect_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    scope_stack: ScopeStack<'tcx>,
    _config: &ResolveOptions,
) -> &'tcx Scope<'tcx> {
    let cc = unit.context();
    let arena = cc.arena();
    let unit_globals_val = Scope::new(HirId(unit.index()));
    let scope_id = unit_globals_val.id().0;
    let unit_globals = arena.alloc_with_id(scope_id, unit_globals_val);
    let mut ctxt = CollectCtxt::new(cc, unit.index(), scope_stack, unit_globals);

    let mut visit = CollectorVisitor::new();
    visit.visit_node(&unit, node, &mut ctxt, unit_globals, None);

    unit_globals
}
