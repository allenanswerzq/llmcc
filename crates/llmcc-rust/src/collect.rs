use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirId, HirIdent, HirNode, HirScope};
use llmcc_core::next_hir_id;
use llmcc_core::scope::{Scope, ScopeStack};
use llmcc_core::symbol::{ScopeId, SymKind, SymKindSet, Symbol};
use llmcc_resolver::{CollectorScopes, ResolverOption};

use std::collections::HashMap;

use crate::LangRust;
use crate::token::AstVisitorRust;

/// Check if a function is in a method context (parent is a type: Struct, Enum, Trait, or UnresolvedType).
/// This is used to distinguish between free functions and methods inside impl blocks.
fn is_method_context(parent: Option<&Symbol>) -> bool {
    parent.is_some_and(|p| {
        matches!(
            p.kind(),
            SymKind::Struct | SymKind::Enum | SymKind::Trait | SymKind::UnresolvedType
        )
    })
}

/// Callback type for scope entry actions
type ScopeEntryCallback<'tcx> = Box<dyn FnOnce(&HirNode<'tcx>, &mut CollectorScopes<'tcx>) + 'tcx>;

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
        // try to find identifier by field first, if not found try scope's identifier
        let ident = node
            .ident_by_field(unit, field_id)
            .or_else(|| node.as_scope().and_then(|sn| sn.opt_ident()))?;

        let sym = scopes.lookup_or_insert(ident.name, node, kind)?;
        ident.set_symbol(sym);

        // Also set the ident on the scope so set_block_id can find it
        if let Some(sn) = node.as_scope() {
            sn.set_ident(ident);
        }

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
        for &child_id in node.child_ids() {
            let child = unit.hir_node(child_id);
            Self::collect_pattern_identifiers_impl(unit, &child, scopes, kind, symbols);
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

    /// Lookup a symbol by name, trying primary kind first, then UnresolvedType, then inserting new
    fn lookup_or_convert(
        &mut self,
        unit: &CompileUnit<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        name: &str,
        node: &HirNode<'tcx>,
        kind: SymKind,
    ) -> Option<&'tcx Symbol> {
        if let Some(symbol) = scopes.lookup_symbol(name, SymKindSet::from_kind(kind)) {
            return Some(symbol);
        }

        if let Some(symbol) =
            scopes.lookup_symbol(name, SymKindSet::from_kind(SymKind::UnresolvedType))
        {
            symbol.set_kind(kind);
            return Some(symbol);
        }

        if let Some(symbol) = scopes.lookup_or_insert(name, node, kind) {
            if symbol.opt_scope().is_none() {
                let scope = self.alloc_scope(unit, symbol);
                symbol.set_scope(scope.id());
            }
            return Some(symbol);
        }

        None
    }

    /// AST: Any scoped node (module, function, trait, impl, etc.)
    /// Purpose: Set up scope hierarchy, link identifiers to symbols, and push/pop scopes
    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(skip_all)]
    fn visit_with_scope(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        sym: &'tcx Symbol,
        sn: &'tcx HirScope<'tcx>,
        ident: &'tcx HirIdent<'tcx>,
        on_scope_enter: Option<ScopeEntryCallback<'tcx>>,
    ) {
        ident.set_symbol(sym);
        sn.set_ident(ident);

        let scope = if sym.opt_scope().is_none() {
            self.alloc_scope(unit, sym)
        } else {
            self.get_scope(sym.scope())
                .unwrap_or_else(|| self.alloc_scope(unit, sym))
        };
        sym.set_scope(scope.id());
        sn.set_scope(scope);

        scopes.push_scope(scope);
        if let Some(callback) = on_scope_enter {
            callback(node, scopes);
        }
        self.visit_children(unit, node, scopes, scope, Some(sym));
        scopes.pop_scope();
    }

    /// AST: Generic scoped-named item handler (module, function, struct, enum, trait, macro, etc.)
    /// Purpose: Declare a named symbol with scope, lookup or insert it, and establish scope hierarchy
    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(skip_all)]
    fn visit_scoped_named(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
        kind: SymKind,
        field_id: u16,
        on_scope_enter: Option<ScopeEntryCallback<'tcx>>,
    ) {
        if let Some((sn, ident)) = node.scope_and_ident_by_field(unit, field_id)
            && let Some(sym) = self.lookup_or_convert(unit, scopes, ident.name, node, kind)
        {
            self.visit_with_scope(unit, node, scopes, sym, sn, ident, on_scope_enter);
        }
    }
}

impl<'tcx> AstVisitorRust<'tcx, CollectorScopes<'tcx>> for CollectorVisitor<'tcx> {
    // Note: Test items (#[test] functions, #[cfg(test)] modules) are already filtered out
    // at the HIR building stage in ir_builder.rs, so they won't appear in the HIR tree.

    /// AST: block { ... }
    /// Purpose: Create a new lexical scope for block-scoped variables and statements
    #[tracing::instrument(skip_all)]
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

    fn visit_match_block(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_block(unit, node, scopes, namespace, parent);
    }

    /// AST: source_file - root node of the compilation unit
    /// Purpose: Parse crate/module names, create file scope, set up global symbol namespace
    #[rustfmt::skip]
    #[tracing::instrument(skip_all)]
    fn visit_source_file(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let start_depth = scopes.scope_depth();
        let meta = unit.unit_meta();

        // Track crate scope for parent relationships
        let mut crate_scope: Option<&'tcx Scope<'tcx>> = None;

        // Set up crate scope from unit metadata
        if let Some(ref crate_name) = meta.package_name
            && let Some(symbol) = scopes.lookup_or_insert_global(crate_name, node, SymKind::Crate)
        {
            scopes.push_scope_with(node, Some(symbol));
            crate_scope = scopes.top();
        }

        // For files in subdirectories (like utils/helper.rs), create a module scope
        // for proper hierarchy traversal. The module scope has a Module symbol.
        let mut module_wrapper_scope: Option<&'tcx Scope<'tcx>> = None;
        if let Some(ref module_name) = meta.module_name
            && let Some(module_sym) = scopes.lookup_or_insert_global(module_name, node, SymKind::Module)
        {
            let mod_scope = self.alloc_scope(unit, module_sym);
            if let Some(crate_s) = crate_scope {
                mod_scope.add_parent(crate_s);
            }
            module_wrapper_scope = Some(mod_scope);
            // Note: We don't change module_sym.set_scope() - that stays pointing to
            // the file scope for path resolution (e.g., `::helper` resolves in utils.rs scope)
        }

        let sn = node.as_scope().unwrap();
        if let Some(ref file_name) = meta.file_name
            && let Some(file_sym) = scopes.lookup_or_insert(file_name, node, SymKind::File)
        {
            let arena_name = unit.cc.arena().alloc_str(file_name);
            let ident = unit.cc.alloc_file_ident(next_hir_id(), arena_name, file_sym);
            ident.set_symbol(file_sym);
            sn.set_ident(ident);

            let scope = self.alloc_scope(unit, file_sym);
            file_sym.set_scope(scope.id());
            sn.set_scope(scope);

            // Add crate and module scopes as parents for hierarchy traversal
            if let Some(crate_s) = crate_scope {
                scope.add_parent(crate_s);
            }
            if let Some(module_s) = module_wrapper_scope {
                scope.add_parent(module_s);
            }

            scopes.push_scope(scope);

            if let Some(crate_sym) = scopes.lookup_or_insert_global("crate", node, SymKind::Module) {
                crate_sym.set_scope(scopes.scopes().globals().id());
            }

            // For top-level files (not lib.rs or main.rs), create a module symbol
            // in the crate scope that links to this file's scope.
            // This enables paths like `crate::models::Config` to resolve.
            // Also insert into the crate scope for cross-crate qualified path resolution
            // (e.g., `crate_b::utils::helper` needs `utils` in `crate_b`'s scope).
            if file_name != "lib" && file_name != "main" && module_wrapper_scope.is_none() {
                if let Some(mod_sym) = scopes.insert_in_global(file_name, node, SymKind::Module) {
                    mod_sym.set_scope(scope.id());
                }
                if let Some(crate_s) = crate_scope
                    && let Some(mod_sym) =
                        scopes.insert_in_scope(crate_s, file_name, node, SymKind::Module)
                {
                    mod_sym.set_scope(scope.id());
                }
            }
        }

        // Use visit_children which handles test filtering automatically
        self.visit_children(unit, node, scopes, namespace, parent);

        scopes.pop_until(start_depth);
    }

    /// AST: mod name { ... } or mod name;
    /// Purpose: Create namespace scope for module, declare module symbol
    #[tracing::instrument(skip_all)]
    fn visit_mod_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if node.child_by_field(unit, LangRust::field_body).is_none() {
            return;
        }

        // Use the current top of the stack (actual parent scope for super::)
        let parent_scope_id = scopes.top().map(|s| s.id()).unwrap_or(namespace.id());

        // Callback to insert `super` symbol pointing to parent module scope
        let on_scope_enter: ScopeEntryCallback<'tcx> = Box::new(move |node, scopes| {
            if let Some(super_sym) = scopes.lookup_or_insert("super", node, SymKind::Module) {
                super_sym.set_scope(parent_scope_id);
            }
        });

        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            SymKind::Namespace,
            LangRust::field_name,
            Some(on_scope_enter),
        );
    }

    /// AST: fn name(...) -> Type { ... }
    /// Purpose: Declare function symbol, create function scope for parameters and body
    #[tracing::instrument(skip_all)]
    fn visit_function_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Determine if this is a method (inside impl) or a free function
        let is_method = is_method_context(parent);
        let kind = if is_method {
            SymKind::Method
        } else {
            SymKind::Function
        };

        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            kind,
            LangRust::field_name,
            None,
        );

        // For free functions, also add a reference in unit_globals for cross-crate resolution
        // This happens after visit_scoped_named so the symbol already exists with its scope
        if !is_method
            && let Some((_, ident)) = node.scope_and_ident_by_field(unit, LangRust::field_name)
            && let Some(sym) = scopes.lookup_symbol(ident.name, SymKindSet::from_kind(kind))
        {
            scopes.globals().insert(sym);
        }
    }

    /// AST: extern "C" fn signature or trait method signature
    /// Purpose: Declare function symbol for extern/trait function signatures
    #[tracing::instrument(skip_all)]
    fn visit_function_signature_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Trait method signatures are also methods
        let kind = if is_method_context(parent) {
            SymKind::Method
        } else {
            SymKind::Function
        };
        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            kind,
            LangRust::field_name,
            None,
        );
    }

    /// AST: struct Name { fields... } or struct Name(types...);
    /// Purpose: Declare struct symbol, create struct scope for fields and methods
    #[tracing::instrument(skip_all)]
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
            Some(Box::new(|node, scopes| {
                let _ = scopes.lookup_or_insert("self", node, SymKind::TypeAlias);
                let _ = scopes.lookup_or_insert("Self", node, SymKind::TypeAlias);
            })),
        );

        // Also add struct to unit_globals for cross-crate type resolution
        if let Some((_, ident)) = node.scope_and_ident_by_field(unit, LangRust::field_name)
            && let Some(sym) =
                scopes.lookup_symbol(ident.name, SymKindSet::from_kind(SymKind::Struct))
        {
            scopes.globals().insert(sym);
        }
    }

    /// AST: enum Name { variants... }
    /// Purpose: Declare enum symbol, create enum scope for variants
    #[tracing::instrument(skip_all)]
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
            None,
        );

        // Also add enum to unit_globals for cross-crate type resolution
        if let Some((_, ident)) = node.scope_and_ident_by_field(unit, LangRust::field_name)
            && let Some(sym) =
                scopes.lookup_symbol(ident.name, SymKindSet::from_kind(SymKind::Enum))
        {
            scopes.globals().insert(sym);
        }
    }

    /// AST: trait Name { associated items... }
    /// Purpose: Declare trait symbol, create trait scope for methods and associated types
    #[tracing::instrument(skip_all)]
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
            Some(Box::new(|node, scopes| {
                let _ = scopes.lookup_or_insert("self", node, SymKind::TypeAlias);
                let _ = scopes.lookup_or_insert("Self", node, SymKind::TypeAlias);
            })),
        );

        // Also add trait to unit_globals for cross-crate type resolution
        if let Some((_, ident)) = node.scope_and_ident_by_field(unit, LangRust::field_name)
            && let Some(sym) =
                scopes.lookup_symbol(ident.name, SymKindSet::from_kind(SymKind::Trait))
        {
            scopes.globals().insert(sym);
        }
    }

    /// AST: impl [Trait for] Type { methods... }
    /// Purpose: Create impl scope for methods
    #[tracing::instrument(skip_all)]
    fn visit_impl_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        // For impl trait references, first try Trait, then UnresolvedType for cross-file resolution
        if let Some(ti) = node.ident_by_field(unit, LangRust::field_trait) {
            // First try looking up as Trait (in-file case)
            let symbol = scopes
                .lookup_symbol(ti.name, SymKindSet::from_kind(SymKind::Trait))
                // Then try looking up as UnresolvedType (existing placeholder)
                .or_else(|| {
                    scopes.lookup_symbol(ti.name, SymKindSet::from_kind(SymKind::UnresolvedType))
                })
                // Finally create UnresolvedType placeholder for cross-file resolution during binding
                .or_else(|| scopes.lookup_or_insert(ti.name, node, SymKind::UnresolvedType));
            if let Some(symbol) = symbol {
                ti.set_symbol(symbol);
            }
        }

        if let Some((sn, ti)) = node.scope_and_ident_by_field(unit, LangRust::field_type)
            && let Some(symbol) =
                self.lookup_or_convert(unit, scopes, ti.name, node, SymKind::UnresolvedType)
        {
            ti.set_symbol(symbol);
            self.visit_with_scope(unit, node, scopes, symbol, sn, ti, None);
        }
    }

    /// AST: macro_rules! name { ... }
    /// Purpose: Declare macro symbol for later macro invocation resolution
    #[tracing::instrument(skip_all)]
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
            None,
        );
    }

    /// AST: const NAME: Type = value;
    /// Purpose: Declare const symbol and visit initializer expression for dependencies
    #[tracing::instrument(skip_all)]
    fn visit_const_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        if let Some(symbol) =
            self.declare_symbol(unit, node, scopes, SymKind::Const, LangRust::field_name)
        {
            self.visit_children(unit, node, scopes, namespace, Some(symbol));
        }
    }

    /// AST: static NAME: Type = value;
    /// Purpose: Declare static symbol and visit initializer expression for dependencies
    #[tracing::instrument(skip_all)]
    fn visit_static_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        if let Some(symbol) =
            self.declare_symbol(unit, node, scopes, SymKind::Static, LangRust::field_name)
        {
            self.visit_children(unit, node, scopes, namespace, Some(symbol));
        }
    }

    /// AST: type Name = AnotherType;
    /// Purpose: Declare type alias symbol and visit the aliased type for dependencies
    #[tracing::instrument(skip_all)]
    fn visit_type_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        if let Some(symbol) =
            self.declare_symbol(unit, node, scopes, SymKind::TypeAlias, LangRust::field_name)
        {
            self.visit_children(unit, node, scopes, namespace, Some(symbol));
        }
    }

    /// AST: Generic type parameter T or K in fn<T, K>(...) or struct<T> { ... }
    /// Purpose: Declare type parameter symbol within generic scope
    #[tracing::instrument(skip_all)]
    fn visit_type_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let _ = self.declare_symbol(
            unit,
            node,
            scopes,
            SymKind::TypeParameter,
            LangRust::field_name,
        );
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    /// AST: Generic const parameter N in fn<const N: usize>(...) or struct<const N: usize> { ... }
    /// Purpose: Declare const parameter symbol and add dependency to owner
    #[tracing::instrument(skip_all)]
    fn visit_const_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let _ = self.declare_symbol(unit, node, scopes, SymKind::Const, LangRust::field_name);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    /// AST: type Assoc = Type; in trait definition
    /// Purpose: Declare associated type symbol within trait scope
    #[tracing::instrument(skip_all)]
    fn visit_associated_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let _ = self.declare_symbol(unit, node, scopes, SymKind::TypeAlias, LangRust::field_name);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    /// AST: where T: Trait, U: Send, ... in generic bounds
    /// Purpose: Visit where clause bounds for type dependency tracking
    #[tracing::instrument(skip_all)]
    fn visit_where_predicate(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // let _ = self.declare_symbol(unit, node, scopes, SymKind::Field, LangRust::field_name);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    /// AST: [Type; N] or [Type]
    /// Purpose: visit array type element and length for dependency tracking
    #[tracing::instrument(skip_all)]
    fn visit_array_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let _ = self.declare_symbol(
            unit,
            node,
            scopes,
            SymKind::CompositeType,
            LangRust::field_name,
        );
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    /// AST: (Type1, Type2, ...) tuple type
    /// Purpose: Visit tuple element types for dependency tracking
    #[tracing::instrument(skip_all)]
    fn visit_tuple_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_array_type(unit, node, scopes, namespace, parent);
    }

    /// AST: i32, u64, f32, bool, str, etc. - primitive type keyword
    /// Purpose: Visit primitive type children (minimal, mostly a no-op)
    #[tracing::instrument(skip_all)]
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

    /// AST: dyn Trait or impl Trait or other advanced type constructs
    /// Purpose: Visit abstract type children for trait dependency tracking
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

    /// AST: field_name: Type in struct body
    /// Purpose: Declare field symbol and visit field type for dependencies
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

    /// AST: Variant in enum { Variant, Variant(Type), Variant { field: Type }, ... }
    /// Purpose: Declare enum variant symbol and link it to parent enum via type_of
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
            None,
        );

        // Set type_of on the variant to point to the parent enum
        if let Some(enum_sym) = parent_enum
            && let Some(ident) = node.ident_by_field(unit, LangRust::field_name)
            && let Some(variant_sym) =
                scopes.lookup_symbol(ident.name, SymKindSet::from_kind(SymKind::EnumVariant))
        {
            variant_sym.set_type_of(enum_sym.id);
        }
    }

    /// AST: parameter in function signature param: Type or pattern, Type in closure
    /// Purpose: Declare function/closure parameter as variable symbol
    fn visit_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        // Check if this is a 'self' parameter
        if let Some(ident) = node.ident_by_field(unit, LangRust::field_pattern)
            && ident.name == "self"
            && let Some(symbol) =
                scopes.lookup_symbol(ident.name, SymKindSet::from_kind(SymKind::Field))
        {
            // For 'self' parameters, try to resolve it as a Field in the current scope
            ident.set_symbol(symbol);
            self.visit_children(unit, node, scopes, namespace, Some(symbol));
            return;
        }

        // Get the pattern node to check if it's a complex pattern (tuple, struct, etc.)
        if let Some(pattern) = node.child_by_field(unit, LangRust::field_pattern) {
            // Check if this is a simple identifier pattern or a complex pattern
            if pattern.as_ident().is_some() {
                // Simple identifier pattern: declare as variable directly
                if let Some(symbol) = self.declare_symbol(
                    unit,
                    node,
                    scopes,
                    SymKind::Variable,
                    LangRust::field_pattern,
                ) {
                    self.visit_children(unit, node, scopes, namespace, Some(symbol));
                    return;
                }
            } else {
                // Complex pattern (tuple, struct, etc.): collect all identifiers
                let _ =
                    Self::collect_pattern_identifiers(unit, &pattern, scopes, SymKind::Variable);
                self.visit_children(unit, node, scopes, namespace, None);
                return;
            }
        }

        // Fallback: try to declare using the old method
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

    /// AST: |param1, param2| { body } - closure/anonymous function
    /// Purpose: Create closure scope, declare closure parameters as variables
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
            if let Some(params) = node.child_by_field(unit, LangRust::field_parameters) {
                let _ = Self::collect_pattern_identifiers(unit, &params, scopes, SymKind::Variable);
            }

            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        }
    }

    /// AST: let pattern = value; or let pattern: Type = value; statement
    /// Purpose: Collect pattern identifiers as variables, handle closure special case
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
            .child_by_field(unit, LangRust::field_value)
            .map(|v| v.kind_id() == LangRust::closure_expression)
            .unwrap_or(false);

        let kind = if is_closure {
            SymKind::Closure
        } else {
            SymKind::Variable
        };

        // Collect the pattern identifier(s) with appropriate kind
        let let_syms = if let Some(pattern) = node.child_by_field(unit, LangRust::field_pattern) {
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
    let unit_globals_val = Scope::new(HirId(unit.index));
    let scope_id = unit_globals_val.id().0;
    let unit_globals = arena.alloc_with_id(scope_id, unit_globals_val);
    let mut scopes = CollectorScopes::new(cc, unit.index, scope_stack, unit_globals);

    let mut visit = CollectorVisitor::new();
    visit.visit_node(&unit, node, &mut scopes, unit_globals, None);

    unit_globals
}
