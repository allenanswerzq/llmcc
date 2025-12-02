use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirId, HirIdent, HirKind, HirNode, HirScope};
use llmcc_core::scope::{Scope, ScopeStack};
use llmcc_core::symbol::{ScopeId, SymKind, Symbol};
use llmcc_core::{next_hir_id, scope};
use llmcc_resolver::{CollectorScopes, ResolverOption};

use std::collections::HashMap;

use crate::LangRust;
use crate::token::AstVisitorRust;
use crate::util::{parse_crate_name, parse_file_name, parse_module_name};

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
        let ident = node.child_ident_by_field(*unit, field_id)?;
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

    /// Lookup a symbol by name, trying primary kind first, then UnresolvedType, then inserting new
    fn lookup_or_convert(
        &mut self,
        unit: &CompileUnit<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        name: &str,
        node: &HirNode<'tcx>,
        primary_kind: SymKind,
    ) -> Option<&'tcx Symbol> {
        tracing::trace!(
            "looking up or converting symbol '{}' of kind {:?}",
            name,
            primary_kind
        );

        // Try looking up by primary kind
        if let Some(symbol) = scopes.lookup_symbol(name, vec![primary_kind]) {
            tracing::trace!(
                "found existing symbol '{}' of kind {:?} {:?}",
                name,
                primary_kind,
                symbol,
            );
            return Some(symbol);
        }

        // Try unresolved type if not found
        if let Some(symbol) = scopes.lookup_symbol(name, vec![SymKind::UnresolvedType]) {
            tracing::trace!(
                "found existing unresolved symbol '{}', converting to {:?}, {:?}",
                name,
                primary_kind,
                symbol,
            );
            symbol.set_kind(primary_kind);
            return Some(symbol);
        }

        // Insert new symbol with primary kind
        if let Some(symbol) = scopes.lookup_or_insert(name, node, primary_kind) {
            tracing::trace!("inserting new symbol '{}' of kind {:?}", name, primary_kind);
            // create a scope for this symbol if needed
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
        if let Some(callback) = on_scope_enter {
            callback(node, scopes);
        }
        self.visit_children(unit, node, scopes, scope, Some(sym));
        scopes.pop_scope();
    }

    /// AST: Generic scoped-named item handler (module, function, struct, enum, trait, macro, etc.)
    /// Purpose: Declare a named symbol with scope, lookup or insert it, and establish scope hierarchy
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
        on_scope_enter: Option<ScopeEntryCallback<'tcx>>,
    ) {
        if let Some((sn, ident)) = node.scope_and_ident_by_field(*unit, field_id) {
            tracing::trace!(
                "visiting scoped named node with kind '{:?}' '{}'",
                kind,
                ident.name
            );
            if let Some(sym) = self.lookup_or_convert(unit, scopes, &ident.name, node, kind) {
                self.visit_with_scope(unit, node, scopes, sym, sn, ident, on_scope_enter);
            }
        } else {
            tracing::warn!("scoped named node is missing scope or ident, skipping");
        }
    }
}

impl<'tcx> AstVisitorRust<'tcx, CollectorScopes<'tcx>> for CollectorVisitor<'tcx> {
    /// AST: block { ... }
    /// Purpose: Create a new lexical scope for block-scoped variables and statements
    fn visit_block(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        tracing::trace!("visiting block");
        if let Some(sn) = node.as_scope() {
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);

            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        }
    }

    /// AST: source_file - root node of the compilation unit
    /// Purpose: Parse crate/module names, create file scope, set up global symbol namespace
    #[rustfmt::skip]
    fn visit_source_file(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        tracing::trace!("visiting source_file");
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

    /// AST: mod name { ... } or mod name;
    /// Purpose: Create namespace scope for module, declare module symbol
    fn visit_mod_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        tracing::trace!("visiting mod_item");
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
            None,
        );
    }

    /// AST: fn name(...) -> Type { ... }
    /// Purpose: Declare function symbol, create function scope for parameters and body
    fn visit_function_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        tracing::trace!("visiting function_item");
        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            SymKind::Function,
            LangRust::field_name,
            None,
        );
    }

    /// AST: extern "C" fn signature or trait method signature
    /// Purpose: Declare function symbol for extern/trait function signatures
    fn visit_function_signature_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        tracing::trace!("visiting function_signature_item");
        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            SymKind::Function,
            LangRust::field_name,
            None,
        );
    }

    /// AST: struct Name { fields... } or struct Name(types...);
    /// Purpose: Declare struct symbol, create struct scope for fields and methods
    fn visit_struct_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        tracing::trace!("visiting struct_item");
        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            SymKind::Struct,
            LangRust::field_name,
            Some(Box::new(|node, scopes| {
                let _ = scopes.lookup_or_insert(&"self", node, SymKind::TypeAlias);
                let _ = scopes.lookup_or_insert(&"Self", node, SymKind::TypeAlias);
            })),
        );
    }

    /// AST: enum Name { variants... }
    /// Purpose: Declare enum symbol, create enum scope for variants
    fn visit_enum_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        tracing::trace!("visiting enum_item");
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
    }

    /// AST: trait Name { associated items... }
    /// Purpose: Declare trait symbol, create trait scope for methods and associated types
    fn visit_trait_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        tracing::trace!("visiting trait_item");
        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            SymKind::Trait,
            LangRust::field_name,
            None,
        );
    }

    /// AST: impl [Trait for] Type { methods... }
    /// Purpose: Create impl scope for methods
    fn visit_impl_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        tracing::trace!("visiting impl_item");
        if let Some(ti) = node.child_ident_by_field(*unit, LangRust::field_trait) {
            if let Some(symbol) =
                self.lookup_or_convert(unit, scopes, &ti.name, node, SymKind::Trait)
            {
                ti.set_symbol(symbol);
            }
        }

        if let Some((sn, ti)) = node.scope_and_ident_by_field(*unit, LangRust::field_type) {
            if let Some(symbol) =
                self.lookup_or_convert(unit, scopes, &ti.name, node, SymKind::Struct)
            {
                ti.set_symbol(symbol);
                self.visit_with_scope(unit, node, scopes, symbol, sn, ti, None);
                return;
            }
        }
    }

    /// AST: macro_rules! name { ... }
    /// Purpose: Declare macro symbol for later macro invocation resolution
    fn visit_macro_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        tracing::trace!("visiting macro_definition");
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
    fn visit_const_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        tracing::trace!("visiting const_item");
        if let Some(symbol) =
            self.declare_symbol(unit, node, scopes, SymKind::Const, LangRust::field_name)
        {
            self.visit_children(unit, node, scopes, namespace, Some(symbol));
        }
    }

    /// AST: static NAME: Type = value;
    /// Purpose: Declare static symbol and visit initializer expression for dependencies
    fn visit_static_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        tracing::trace!("visiting static_item");
        if let Some(symbol) =
            self.declare_symbol(unit, node, scopes, SymKind::Static, LangRust::field_name)
        {
            self.visit_children(unit, node, scopes, namespace, Some(symbol));
        }
    }

    /// AST: type Name = AnotherType;
    /// Purpose: Declare type alias symbol and visit the aliased type for dependencies
    fn visit_type_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        tracing::trace!("visiting type_item");
        if let Some(symbol) =
            self.declare_symbol(unit, node, scopes, SymKind::TypeAlias, LangRust::field_name)
        {
            self.visit_children(unit, node, scopes, namespace, Some(symbol));
        }
    }

    /// AST: Identifier used in type context (e.g., type annotation, generics)
    /// Purpose: Resolve type identifier to struct/enum/trait/function/type-alias symbol
    fn visit_type_identifier(
        &mut self,
        _unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        tracing::trace!("visiting type_identifier");
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

    /// AST: Generic type parameter T or K in fn<T, K>(...) or struct<T> { ... }
    /// Purpose: Declare type parameter symbol within generic scope
    fn visit_type_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        tracing::trace!("visiting type_parameter");
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
    fn visit_const_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        tracing::trace!("visiting const_parameter");
        let _ = self.declare_symbol(unit, node, scopes, SymKind::Const, LangRust::field_name);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    /// AST: type Assoc = Type; in trait definition
    /// Purpose: Declare associated type symbol within trait scope
    fn visit_associated_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        tracing::trace!("visiting associated_type");
        let _ = self.declare_symbol(unit, node, scopes, SymKind::TypeAlias, LangRust::field_name);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    /// AST: where T: Trait, U: Send, ... in generic bounds
    /// Purpose: Visit where clause bounds for type dependency tracking
    fn visit_where_predicate(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        tracing::trace!("visiting where_predicate");
        let _ = self.declare_symbol(unit, node, scopes, SymKind::Field, LangRust::field_name);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    /// AST: [Type; N] or [Type]
    /// Purpose: Visit array type element and length for dependency tracking
    fn visit_array_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        tracing::trace!("visiting array_type");
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    /// AST: (Type1, Type2, ...) tuple type
    /// Purpose: Visit tuple element types for dependency tracking
    fn visit_tuple_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        tracing::trace!("visiting tuple_type");
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    /// AST: i32, u64, f32, bool, str, etc. - primitive type keyword
    /// Purpose: Visit primitive type children (minimal, mostly a no-op)
    fn visit_primitive_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        tracing::trace!("visiting primitive_type");
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
        tracing::trace!("visiting abstract_type");
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
        tracing::trace!("visiting field_declaration");
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
        tracing::trace!("visiting enum_variant");
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
            && let Some(ident) = node.child_ident_by_field(*unit, LangRust::field_name)
            && let Some(variant_sym) = scopes.lookup_symbol(&ident.name, vec![SymKind::EnumVariant])
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
        parent: Option<&Symbol>,
    ) {
        tracing::trace!("visiting parameter");

        // Check if this is a 'self' parameter
        if let Some(ident) = node.child_ident_by_field(*unit, LangRust::field_pattern) {
            if ident.name == "self" {
                // For 'self' parameters, try to resolve it as a Field in the current scope
                if let Some(symbol) = scopes.lookup_symbol(&ident.name, vec![SymKind::Field]) {
                    ident.set_symbol(symbol);
                    self.visit_children(unit, node, scopes, namespace, Some(symbol));
                    return;
                }
            }
        }

        // For non-self parameters, declare as Variable
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
        tracing::trace!("visiting closure_expression");
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
        tracing::trace!("visiting let_declaration");
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
    use textwrap::dedent;

    fn with_compiled_unit<F>(sources: &[&str], check: F)
    where
        F: for<'a> FnOnce(&'a CompileCtxt<'a>),
    {
        // Initialize tracing for test output
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_test_writer()
            .try_init();

        let bytes = sources
            .iter()
            .map(|src| src.as_bytes().to_vec())
            .collect::<Vec<_>>();

        let cc = CompileCtxt::from_sources::<LangRust>(&bytes);
        build_llmcc_ir::<LangRust>(&cc, IrBuildOption::default()).unwrap();

        let resolver_option = ResolverOption::default()
            .with_sequential(true)
            .with_print_ir(true);
        let _globals = collect_symbols_with::<LangRust>(&cc, &resolver_option);
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

    #[serial_test::serial]
    #[test]
    fn visit_mod_item_declares_namespace() {
        let source = dedent(
            "
            mod utils {
                pub fn helper() {}
            }
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "utils", SymKind::Namespace).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_function_item_declares_function() {
        let source = dedent(
            "
            fn my_function() {
                let x = 42;
            }
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "my_function", SymKind::Function).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_struct_item_declares_struct() {
        let source = dedent(
            "
            struct Person {
                name: String,
                age: u32,
            }
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "Person", SymKind::Struct).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_enum_item_declares_enum() {
        let source = dedent(
            "
            enum Color {
                Red,
                Green,
                Blue,
            }
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "Color", SymKind::Enum).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_trait_item_declares_trait() {
        let source = dedent(
            "
            trait Drawable {
                fn draw(&self);
            }
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "Drawable", SymKind::Trait).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_const_item_declares_const() {
        let source = dedent(
            "
            const MAX_SIZE: usize = 100;
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "MAX_SIZE", SymKind::Const).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_static_item_declares_static() {
        let source = dedent(
            "
            static GLOBAL_VAR: i32 = 42;
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "GLOBAL_VAR", SymKind::Static).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_type_item_declares_type_alias() {
        let source = dedent(
            "
            type MyResult<T> = Result<T, String>;
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "MyResult", SymKind::TypeAlias).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_field_declaration_declares_field() {
        let source = dedent(
            "
            struct Point {
                x: i32,
                y: i32,
            }
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "x", SymKind::Field).0 > 0);
            assert!(find_symbol_id(cc, "y", SymKind::Field).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_enum_variant_declares_variant() {
        let source = dedent(
            "
            enum Status {
                Active,
                Inactive,
            }
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "Active", SymKind::EnumVariant).0 > 0);
            assert!(find_symbol_id(cc, "Inactive", SymKind::EnumVariant).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_parameter_declares_parameter() {
        let source = dedent(
            "
            fn add(a: i32, b: i32) -> i32 {
                a + b
            }
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "a", SymKind::Variable).0 > 0);
            assert!(find_symbol_id(cc, "b", SymKind::Variable).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_let_declaration_declares_variable() {
        let source = dedent(
            "
            fn create_value() {
                let value = 42;
                let another = \"hello\";
            }
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "value", SymKind::Variable).0 > 0);
            assert!(find_symbol_id(cc, "another", SymKind::Variable).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_closure_expression_declares_closure() {
        let source = dedent(
            "
            fn use_closure() {
                let square = |x| x * x;
                let result = square(5);
            }
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "square", SymKind::Closure).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_type_parameter_declares_type_param() {
        let source = dedent(
            "
            fn generic<T>(value: T) -> T {
                value
            }
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "T", SymKind::TypeParameter).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_const_parameter_declares_const_param() {
        let source = dedent(
            "
            fn with_const<const N: usize>() -> usize {
                N
            }
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "N", SymKind::Const).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_associated_type_in_trait() {
        let source = dedent(
            "
            trait MyIterator {
                type Item;
                fn next(&mut self) -> Option<Self::Item>;
            }
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "Item", SymKind::TypeAlias).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_block_creates_scope() {
        let source = dedent(
            "
            fn scope_example() {
                {
                    let inner = 10;
                }
                let outer = 20;
            }
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "outer", SymKind::Variable).0 > 0);
            assert!(find_symbol_id(cc, "inner", SymKind::Variable).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_nested_modules() {
        let source = dedent(
            "
            mod outer {
                pub mod inner {
                    pub fn nested_fn() {}
                }
            }
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "outer", SymKind::Namespace).0 > 0);
            assert!(find_symbol_id(cc, "inner", SymKind::Namespace).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_multiple_struct_fields() {
        let source = dedent(
            "
            struct Config {
                host: String,
                port: u16,
                timeout: u64,
            }
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "host", SymKind::Field).0 > 0);
            assert!(find_symbol_id(cc, "port", SymKind::Field).0 > 0);
            assert!(find_symbol_id(cc, "timeout", SymKind::Field).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_impl_trait_for_type() {
        let source = dedent(
            "
            struct MyType;
            trait MyTrait {
                fn do_something(&self);
            }
            impl MyTrait for MyType {
                fn do_something(&self) {}
            }
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "MyType", SymKind::Struct).0 > 0);
            assert!(find_symbol_id(cc, "MyTrait", SymKind::Trait).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_macro_rules_declares_macro() {
        let source = dedent(
            "
            macro_rules! my_macro {
                () => {
                    42
                };
            }
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "my_macro", SymKind::Macro).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_function_signature_in_trait() {
        let source = dedent(
            "
            trait Calculator {
                fn add(a: i32, b: i32) -> i32;
                fn subtract(x: i32, y: i32) -> i32;
            }
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "add", SymKind::Function).0 > 0);
            assert!(find_symbol_id(cc, "subtract", SymKind::Function).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_generic_struct_with_multiple_params() {
        let source = dedent(
            "
            struct Pair<T, U> {
                first: T,
                second: U,
            }
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            assert!(find_symbol_id(cc, "T", SymKind::TypeParameter).0 > 0);
            assert!(find_symbol_id(cc, "U", SymKind::TypeParameter).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_method_with_self_parameter() {
        let source = dedent(
            "
            struct Counter {
                count: i32,
            }

            impl Counter {
                fn increment(&mut self) {
                    self.count += 1;
                }

                fn get_count(&self) -> i32 {
                    self.count
                }
            }
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            // Verify struct is collected
            let counter_id = find_symbol_id(cc, "Counter", SymKind::Struct);
            assert!(counter_id.0 > 0);

            // Verify methods are collected
            assert!(find_symbol_id(cc, "increment", SymKind::Function).0 > 0);
            assert!(find_symbol_id(cc, "get_count", SymKind::Function).0 > 0);

            // Verify field is collected
            assert!(find_symbol_id(cc, "count", SymKind::Field).0 > 0);

            assert!(find_symbol_id(cc, "self", SymKind::Field).0 > 0);
            assert!(find_symbol_id(cc, "Self", SymKind::TypeAlias).0 > 0);
        });
    }

    #[serial_test::serial]
    #[test]
    fn visit_self_in_different_parameter_forms() {
        let source = dedent(
            "
            struct MyType;

            impl MyType {
                fn by_value(self) {}
                fn by_mut_ref(&mut self) {}
                fn by_ref(&self) {}
            }
            ",
        );
        with_compiled_unit(&[&source], |cc| {
            // Verify struct
            assert!(find_symbol_id(cc, "MyType", SymKind::Struct).0 > 0);

            // Verify all three methods are collected
            assert!(find_symbol_id(cc, "by_value", SymKind::Function).0 > 0);
            assert!(find_symbol_id(cc, "by_mut_ref", SymKind::Function).0 > 0);
            assert!(find_symbol_id(cc, "by_ref", SymKind::Function).0 > 0);
        });
    }
}
