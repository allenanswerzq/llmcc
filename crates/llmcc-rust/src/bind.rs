use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirKind, HirNode, HirScope};
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{DepKind, SymKind, Symbol};
use llmcc_resolver::{BinderScopes, ResolverOption};

use crate::token::AstVisitorRust;
use crate::token::LangRust;
use crate::ty::TyCtxt;
use crate::util::{parse_crate_name, parse_file_name, parse_module_name};

type ScopeEntryCallback<'tcx> =
    Box<dyn FnOnce(&CompileUnit<'tcx>, &'tcx HirScope<'tcx>, &mut BinderScopes<'tcx>) + 'tcx>;

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

    /// Helper function to add dependencies from a symbol to all identifiers in a node.
    /// This handles trait bounds, where clause bounds, type arguments, and function arguments.
    fn collect_type_depends(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        symbol: &Symbol,
        dep_kind: DepKind,
    ) {
        for ident in node.collect_idents(unit) {
            if let Some(ident_sym) = ident.opt_symbol() {
                tracing::trace!(
                    "adding dependency from '{}' to '{}' with kind '{:?}'",
                    symbol.format(Some(unit.interner())),
                    ident_sym.format(Some(unit.interner())),
                    dep_kind,
                );
                symbol.add_depends_with(ident_sym, dep_kind);
            }
        }
    }

    #[tracing::instrument(skip_all)]
    #[allow(clippy::too_many_arguments)]
    fn visit_scoped_named(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        sn: &'tcx HirScope<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
        on_scope_enter: Option<ScopeEntryCallback<'tcx>>,
    ) {
        tracing::trace!(
            "visiting scoped named node kind: {:?}, namespace id: {:?}, parent: {:?}",
            node.kind_id(),
            namespace.id(),
            parent.map(|p| p.format(Some(unit.interner()))),
        );
        let depth = scopes.scope_depth();
        let child_parent = sn
            .opt_ident()
            .and_then(|ident| ident.opt_symbol())
            .or(parent);

        scopes.push_scope_node(sn);
        if let Some(callback) = on_scope_enter {
            callback(unit, sn, scopes);
        }
        self.visit_children(unit, node, scopes, scopes.top(), child_parent);
        scopes.pop_until(depth);

        // namespace owner to curent relationship
        if let Some(ns) = namespace.opt_symbol()
            && let Some(sym) = sn.opt_symbol()
        {
            let dep_kind = DepKind::Uses;
            tracing::trace!(
                "adding depends from namespace '{}' to symbol '{}' '{:?}'",
                ns.format(Some(unit.interner())),
                sym.format(Some(unit.interner())),
                dep_kind,
            );
            ns.add_depends_with(sym, dep_kind);
        }
    }

    /// Recursively collect type dependencies from patterns.
    /// Handles struct patterns (MyStruct { field }), tuple patterns, tuple struct patterns, etc.
    #[allow(clippy::only_used_in_recursion)]
    fn collect_pattern_deps(
        &mut self,
        unit: &CompileUnit<'tcx>,
        pattern_node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
    ) {
        if let Some(ns) = namespace.opt_symbol() {
            let mut ty_ctxt = TyCtxt::new(unit, scopes);

            // Try to resolve the pattern node as a type (for struct patterns, tuple struct patterns, etc.)
            if let Some(type_node) = pattern_node.child_by_field(*unit, LangRust::field_type)
                && let Some(pattern_type) = ty_ctxt.resolve_type(&type_node)
            {
                ns.add_depends_with(pattern_type, DepKind::Uses);
            }

            // For scoped identifiers in patterns (module::Type or E::Variant)
            if pattern_node.kind_id() == LangRust::scoped_identifier
                && let Some(resolved) = ty_ctxt.resolve_type(pattern_node)
            {
                ns.add_depends_with(resolved, DepKind::Uses);
            }

            // Recursively process nested patterns (field patterns, tuple elements, etc.)
            for child in pattern_node.children(unit) {
                // Skip trivia nodes
                if matches!(child.kind(), HirKind::Text | HirKind::Comment) {
                    continue;
                }

                // Recursively handle nested patterns
                self.collect_pattern_deps(unit, &child, scopes, namespace);
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
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let file_path = unit.file_path().unwrap();
        let depth = scopes.scope_depth();

        tracing::trace!("binding source_file: {}", file_path);
        if let Some(scope_id) = parse_crate_name(file_path).and_then(|crate_name| {
            scopes
                .lookup_symbol(&crate_name, vec![SymKind::Crate])
                .and_then(|symbol| symbol.opt_scope())
        }) {
            tracing::trace!("pushing crate scope {:?}", scope_id);
            scopes.push_scope(scope_id);
        }

        if let Some(scope_id) = parse_module_name(file_path).and_then(|module_name| {
            scopes
                .lookup_symbol(&module_name, vec![SymKind::Module])
                .and_then(|symbol| symbol.opt_scope())
        }) {
            tracing::trace!("pushing module scope {:?}", scope_id);
            scopes.push_scope(scope_id);
        }

        if let Some(file_sym) = parse_file_name(file_path)
            .and_then(|file_name| scopes.lookup_symbol(&file_name, vec![SymKind::File]))
            && let Some(scope_id) = file_sym.opt_scope()
        {
            tracing::trace!("pushing file scope {} {:?}", file_path, scope_id);
            scopes.push_scope(scope_id);

            let file_scope = unit.get_scope(scope_id);
            self.visit_children(unit, node, scopes, file_scope, Some(file_sym));
            scopes.pop_until(depth);
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

    #[tracing::instrument(skip_all)]
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
        self.visit_scoped_named(unit, node, sn, scopes, namespace, parent, None);
    }

    #[tracing::instrument(skip_all)]
    fn visit_function_signature_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_function_item(unit, node, scopes, namespace, parent);
    }

    #[tracing::instrument(skip_all)]
    fn visit_function_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let sn = node.as_scope().unwrap();
        self.visit_scoped_named(unit, node, sn, scopes, namespace, parent, None);

        // At this point, all return type node should already be bound
        let ret_ident = node.ident_by_field(*unit, LangRust::field_return_type);

        if let Some(fn_sym) = sn.opt_symbol() {
            // Mark main function as global (entry point)
            if unit.interner().resolve_owned(fn_sym.name).as_deref() == Some("main") {
                tracing::trace!("marking main function as global");
                fn_sym.set_is_global(true);
            }

            // Handle the return type identifier (e.g., Option in Option<UserDto>)
            if let Some(ret_ty) = ret_ident
                && let Some(ret_sym) = ret_ty.opt_symbol()
            {
                fn_sym.set_type_of(ret_sym.id());
                fn_sym.add_depends_with(
                    ret_sym,
                    DepKind::ReturnType,
                );
            }

            // Also try to resolve return type using type inference for complex paths (e.g., crate::Type)
            if let Some(ret_type_node) = node.child_by_field(*unit, LangRust::field_return_type) {
                let mut ty_ctxt = TyCtxt::new(unit, scopes);
                if let Some(resolved_type) = ty_ctxt.resolve_type(&ret_type_node) {
                    // Only set if we haven't already set it via simple identifier
                    if fn_sym.type_of().is_none() {
                        fn_sym.set_type_of(resolved_type.id());
                    }
                    fn_sym.add_depends_with(
                        resolved_type,
                        DepKind::ReturnType,
                    );
                }
            }
        }
    }

    #[tracing::instrument(skip_all)]
    fn visit_primitive_type(
        &mut self,
        _unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let ident = node.as_ident().unwrap();
        if let Some(symbol) = scopes.lookup_global(&ident.name, vec![SymKind::Primitive]) {
            ident.set_symbol(symbol);
        }
    }

    #[tracing::instrument(skip_all)]
    fn visit_type_identifier(
        &mut self,
        _unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let ident = node.as_ident().unwrap();
        if let Some(symbol) = ident.opt_symbol()
            && symbol.kind() != SymKind::UnresolvedType
        {
            return;
        }

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
        }
    }

    #[tracing::instrument(skip_all)]
    fn visit_struct_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let sn = node.as_scope().unwrap();
        self.visit_scoped_named(
            unit,
            node,
            sn,
            scopes,
            namespace,
            parent,
            Some(Box::new(|unit, sn, scopes| {
                // lets bind Self/self to the struct type symbol
                for key in ["Self", "self"] {
                    if let Some(self_sym) = scopes.lookup_symbol(key, vec![SymKind::TypeAlias])
                        && let Some(struct_sym) = sn.opt_symbol()
                    {
                        tracing::trace!(
                            "binding '{}' to struct type '{}'",
                            key,
                            struct_sym.format(Some(unit.interner())),
                        );
                        self_sym.set_type_of(struct_sym.id());
                        // assign scope
                        if let Some(struct_scope) = struct_sym.opt_scope() {
                            self_sym.set_scope(struct_scope);
                        }
                    }
                }
            })),
        );
    }
    #[tracing::instrument(skip_all)]
    fn visit_impl_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let target_ident = node.ident_by_field(*unit, LangRust::field_type);
        if let Some(target_sym) = target_ident.and_then(|ident| ident.opt_symbol()) {
            let target_node = node.child_by_field(*unit, LangRust::field_type).unwrap();
            let target_resolved = TyCtxt::new(unit, scopes).resolve_type(&target_node);

            if target_sym.kind() == SymKind::UnresolvedType {
                // Resolve the type for the impl type now
                if let Some(resolved) = target_resolved
                    && resolved.id() != target_sym.id()
                    && !matches!(resolved.kind(), SymKind::UnresolvedType)
                {
                    tracing::trace!(
                        "resolving impl target type '{}' to '{}'",
                        target_sym.format(Some(unit.interner())),
                        resolved.format(Some(unit.interner())),
                    );
                    // Update the unresolved symbol to point to the actual type
                    target_sym.set_type_of(resolved.id());
                    target_sym.set_kind(resolved.kind());
                    target_sym.add_depends_with(resolved, DepKind::Uses);
                    target_sym.set_is_global(resolved.is_global());

                    if let Some(resolved_scope) = resolved.opt_scope()
                        && let Some(target_scoped) = target_sym.opt_scope()
                    {
                        // Build parent scope relationship
                        tracing::trace!(
                            "connecting impl target '{}' and resolved type '{}'",
                            resolved_scope,
                            target_scoped
                        );
                        let target_scope = unit.get_scope(target_scoped);
                        let resolved_scope = unit.get_scope(resolved_scope);
                        target_scope.add_parent(resolved_scope);
                        resolved_scope.add_parent(target_scope);
                    }
                }
            }

            if let Some(trait_node) = node.child_by_field(*unit, LangRust::field_trait)
                && let Some(trait_sym) = TyCtxt::new(unit, scopes).resolve_type(&trait_node)
                && let Some(target_resolved) = target_resolved
                && let Some(target_scope) = target_resolved.opt_scope()
                && let Some(trait_scope) = trait_sym.opt_scope()
            {
                let target_scope = unit.get_scope(target_scope);
                let trait_scope = unit.get_scope(trait_scope);
                tracing::trace!(
                    "adding impl realtion: target '{}' implements trait '{}'",
                    target_resolved.format(Some(unit.interner())),
                    trait_sym.format(Some(unit.interner())),
                );
                target_scope.add_parent(trait_scope);
                target_resolved.add_depends_with(trait_sym, DepKind::Implements);
            }

            let sn = node.as_scope().unwrap();
            let target_scope = unit.get_scope(target_sym.opt_scope().unwrap());
            self.visit_scoped_named(unit, node, sn, scopes, target_scope, Some(target_sym), None);

            if let Some(arg) = target_node.child_by_field(*unit, LangRust::field_type_arguments) {
                Self::collect_type_depends(unit, &arg, target_sym, DepKind::Uses);
            }
        }
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

        let mut ty_ctxt = TyCtxt::new(unit, scopes);
        let target = ty_ctxt.resolve_callable(node);

        if let Some(target_symbol) = target
            && let Some(ns) = namespace.opt_symbol()
        {
            if target_symbol.kind() == SymKind::EnumVariant
                && let Some(enum_symbol_id) = target_symbol.type_of()
                && let Some(enum_symbol) = unit.opt_get_symbol(enum_symbol_id)
            {
                ns.add_depends_with(enum_symbol, DepKind::Calls);
            } else {
                ns.add_depends_with(
                    target_symbol,
                    DepKind::Calls,
                );
            }
        }

        // For scoped calls like Type::method(), also add depends on the Type
        if let Some(func_node) = node.child_by_field(*unit, LangRust::field_function)
            && func_node.kind_id() == LangRust::scoped_identifier
            && let Some(ns) = namespace.opt_symbol()
            && let Some(path_type) = ty_ctxt.resolve_type(&func_node)
        {
            ns.add_depends_with(path_type, DepKind::Uses);
        }

        // Add depends from call target to nested call targets in arguments
        if let Some(arg) = node.child_by_field(*unit, LangRust::field_arguments)
            && let Some(target_sym) = target
        {
            Self::collect_type_depends(unit, &arg, target_sym, DepKind::Uses);
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
        let sn = node.as_scope().unwrap();
        self.visit_scoped_named(unit, node, sn, scopes, namespace, parent, None);
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
        self.visit_scoped_named(unit, node, sn, scopes, namespace, parent, None);

        if let Some(bounds) = node.child_by_field(*unit, LangRust::field_bounds)
            && let Some(trait_sym) = sn.opt_symbol()
        {
            Self::collect_type_depends(unit, &bounds, trait_sym, DepKind::TypeBound);
        }
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
        self.visit_scoped_named(unit, node, sn, scopes, namespace, parent, None);
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

        if let Some(macro_node) = node.child_by_field(*unit, LangRust::field_macro)
            && let Some(sym) = TyCtxt::new(unit, scopes).resolve_type(&macro_node)
            && let Some(ns) = namespace.opt_symbol()
        {
            ns.add_depends_with(sym, DepKind::Calls);
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
        if let Some(const_ident) = node.ident_by_field(*unit, LangRust::field_name)
            && let Some(const_ty) = node.child_by_field(*unit, LangRust::field_type)
            && let Some(const_sym) = const_ident.opt_symbol()
            && let Some(ty) = TyCtxt::new(unit, scopes).resolve_type(&const_ty)
        {
            const_sym.set_type_of(ty.id());
            const_sym.add_depends_with(ty, DepKind::Uses);
            if let Some(ns) = namespace.opt_symbol() {
                ns.add_depends_with(const_sym, DepKind::Uses);
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

    #[tracing::instrument(skip_all)]
    fn visit_type_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(type_ident) = node.ident_by_field(*unit, LangRust::field_name)
            && let Some(type_sym) = type_ident.opt_symbol()
        {
            tracing::trace!(
                "visiting type alias '{}' for resolution",
                type_sym.format(Some(unit.interner())),
            );

            // Resolve the type that this alias points to
            if let Some(type_node) = node.child_by_field(*unit, LangRust::field_type) {
                let mut ty_ctxt = TyCtxt::new(unit, scopes);
                if let Some(resolved_type) = ty_ctxt.resolve_type(&type_node) {
                    tracing::trace!(
                        "type alias '{}' resolves to '{}'",
                        type_sym.format(Some(unit.interner())),
                        resolved_type.format(Some(unit.interner())),
                    );

                    type_sym.set_type_of(resolved_type.id());
                    type_sym.add_depends_with(
                        resolved_type,
                        DepKind::Alias,
                    );
                }
            }

            // Handle where clauses if present
            for child in node.children(unit) {
                if child.kind_id() == LangRust::where_clause {
                    Self::collect_type_depends(unit, &child, type_sym, DepKind::Uses);
                }
            }
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

        if let Some(bounds) = node.child_by_field(*unit, LangRust::field_bounds)
            && let Some(ns) = namespace.opt_symbol()
        {
            Self::collect_type_depends(unit, &bounds, ns, DepKind::TypeBound);
        }
    }

    fn visit_const_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // Const parameters have a type like `const N: usize`
        if let Some(type_node) = node.child_by_field(*unit, LangRust::field_type)
            && let Some(name_ident) = node.ident_by_field(*unit, LangRust::field_name)
            && let Some(const_sym) = name_ident.opt_symbol()
            && let Some(ty) = TyCtxt::new(unit, scopes).resolve_type(&type_node)
        {
            const_sym.set_type_of(ty.id());
            const_sym.add_depends_with(ty, DepKind::Uses);

            if let Some(ns) = namespace.opt_symbol() {
                ns.add_depends_with(ty, DepKind::Uses);
            }
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

        if let Some(type_ident) = node.ident_by_field(*unit, LangRust::field_name)
            && let Some(type_sym) = type_ident.opt_symbol()
            && let Some(type_node) = node.child_by_field(*unit, LangRust::field_default_type)
        {
            let mut ty_ctxt = TyCtxt::new(unit, scopes);
            if let Some(resolved_type) = ty_ctxt.resolve_type(&type_node) {
                type_sym.set_type_of(resolved_type.id());
                type_sym.add_depends_with(resolved_type, DepKind::Uses);
                if let Some(ns) = namespace.opt_symbol() {
                    ns.add_depends_with(resolved_type, DepKind::Uses);
                }
            }
        }
    }

    fn visit_where_predicate(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // Where predicates add bounds to types. Example: T: Trait + Display
        // The bounds should be added as TypeBound dependencies to the owner
        if let Some(bounds) = node.child_by_field(*unit, LangRust::field_bounds)
            && let Some(ns) = namespace.opt_symbol()
        {
            Self::collect_type_depends(unit, &bounds, ns, DepKind::TypeBound);
        }
    }

    fn visit_array_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // Array types track the element type: [T; N]
        if let Some(element_ty) = node.child_by_field(*unit, LangRust::field_element)
            && let Some(ns) = namespace.opt_symbol()
        {
            let mut ty_ctxt = TyCtxt::new(unit, scopes);
            if let Some(resolved_type) = ty_ctxt.resolve_type(&element_ty) {
                ns.add_depends_with(resolved_type, DepKind::Uses);
            }
        }
    }

    fn visit_tuple_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // Tuple types track all element types: (T1, T2, T3)
        if let Some(ns) = namespace.opt_symbol() {
            let mut ty_ctxt = TyCtxt::new(unit, scopes);

            // Tuple elements are direct children (no named field)
            for child in node.children(unit) {
                // Skip non-type nodes
                if matches!(child.kind(), HirKind::Text | HirKind::Comment) {
                    continue;
                }

                if let Some(resolved_type) = ty_ctxt.resolve_type(&child) {
                    ns.add_depends_with(resolved_type, DepKind::Uses);
                }
            }
        }
    }

    fn visit_abstract_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // Abstract types (impl Trait) track the trait bound: impl Iterator, impl Clone, etc.
        if let Some(trait_node) = node.child_by_field(*unit, LangRust::field_trait)
            && let Some(ns) = namespace.opt_symbol()
        {
            let mut ty_ctxt = TyCtxt::new(unit, scopes);
            if let Some(resolved_trait) = ty_ctxt.resolve_type(&trait_node) {
                ns.add_depends_with(resolved_trait, DepKind::Uses);
            }
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

        // Struct/enum fields track their type dependencies
        let owner_sym = if let Some(sym) = namespace.opt_symbol() {
            sym
        } else if let Some(parent_sym) = parent
            && let Some(resolved) = unit.opt_get_symbol(parent_sym.id())
        {
            resolved
        } else {
            return;
        };

        if let Some(_name_node) = node.ident_by_field(*unit, LangRust::field_name)
            && let Some(type_node) = node.child_by_field(*unit, LangRust::field_type)
        {
            let mut ty_ctxt = TyCtxt::new(unit, scopes);
            if let Some(resolved_type) = ty_ctxt.resolve_type(&type_node) {
                owner_sym.add_depends_with(resolved_type, DepKind::Uses);
            }
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

        // Enum variants track the types of their associated data
        if let Some(name_node) = node.ident_by_field(*unit, LangRust::field_name)
            && let Some(symbol) = name_node.opt_symbol()
            && let Some(ns) = namespace.opt_symbol()
        {
            ns.add_depends_with(symbol, DepKind::Uses);

            // Handle tuple-like variants: Value(i32, String)
            // The types are in the body field
            if let Some(body_node) = node.child_by_field(*unit, LangRust::field_body) {
                // Collect all identifiers in the body (field types)
                for ident in body_node.collect_idents(unit) {
                    if let Some(ident_sym) = ident.opt_symbol() {
                        symbol.add_depends_with(ident_sym, DepKind::Uses);
                        ns.add_depends_with(ident_sym, DepKind::Uses);
                    }
                }
            }
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

        // Parameters track their type dependencies
        if let Some(type_node) = node.child_by_field(*unit, LangRust::field_type) {
            let mut ty_ctxt = TyCtxt::new(unit, scopes);
            if let Some(resolved_type) = ty_ctxt.resolve_type(&type_node) {
                // Add dependency from containing function/method to parameter type
                if let Some(owner) = parent {
                    owner.add_depends_with(resolved_type, DepKind::Uses);
                }

                // Also add from namespace (closure, etc.)
                if let Some(ns) = namespace.opt_symbol() {
                    ns.add_depends_with(resolved_type, DepKind::Uses);
                }
            }
        }
    }

    fn visit_scoped_identifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // Scoped identifiers like E::V1 or E::V2 need to track the parent enum
        if let Some(owner) = namespace.opt_symbol()
            && let Some(sym) = TyCtxt::new(unit, scopes).resolve_type(node)
            && sym.kind() == SymKind::EnumVariant
            && let Some(parent_enum_id) = sym.type_of()
            && let Some(parent_enum) = unit.opt_get_symbol(parent_enum_id)
        {
            owner.add_depends_with(parent_enum, DepKind::Uses);
        }
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

        let mut ty_ctxt = TyCtxt::new(unit, scopes);

        // Get explicit type annotation if present
        let explicit_type =
            if let Some(type_node) = node.child_by_field(*unit, LangRust::field_type) {
                ty_ctxt.resolve_type(&type_node)
            } else {
                None
            };

        // Get inferred type from value expression if present
        let inferred_type = if explicit_type.is_none() {
            if let Some(value_node) = node.child_by_field(*unit, LangRust::field_value) {
                ty_ctxt.resolve_type(&value_node)
            } else {
                None
            }
        } else {
            None
        };

        let resolved_type = explicit_type.or(inferred_type);

        // If we resolved a type, add dependency from the containing scope
        if let Some(ty) = resolved_type
            && let Some(ns) = namespace.opt_symbol()
        {
            ns.add_depends_with(ty, DepKind::Uses);
        }

        // Handle pattern-based dependencies (struct patterns, tuple patterns, etc.)
        if let Some(pattern_node) = node.child_by_field(*unit, LangRust::field_pattern) {
            self.collect_pattern_deps(unit, &pattern_node, scopes, namespace);
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

        // Struct expressions like MyStruct { field: value } depend on the struct type
        if let Some(name_node) = node
            .child_by_field(*unit, LangRust::field_name)
            .or_else(|| node.child_by_field(*unit, LangRust::field_type))
        {
            let mut ty_ctxt = TyCtxt::new(unit, scopes);
            if let Some(struct_type) = ty_ctxt.resolve_type(&name_node)
                && let Some(ns) = namespace.opt_symbol()
            {
                ns.add_depends_with(struct_type, DepKind::Uses);
            }
        }
    }
}

pub fn bind_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    namespace: &'tcx Scope<'tcx>,
    _config: &ResolverOption,
) {
    let mut scopes = BinderScopes::new(unit, namespace);
    let mut visit = BinderVisitor::new();
    visit.visit_node(&unit, node, &mut scopes, namespace, None);
}
