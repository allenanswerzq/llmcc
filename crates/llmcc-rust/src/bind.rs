use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirKind, HirNode, HirScope};
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{DepKind, SymKind, Symbol};
use llmcc_resolver::{BinderScopes, ResolverOption};

use crate::token::AstVisitorRust;
use crate::token::LangRust;
use crate::ty::{self, TyCtxt};
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
                    symbol.format(Some(&unit.interner())),
                    ident_sym.format(Some(&unit.interner())),
                    dep_kind,
                );
                symbol.add_depends_with(ident_sym, dep_kind, Some(&[SymKind::TypeParameter]));
            }
        }
    }

    #[tracing::instrument(skip_all)]
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
            parent.map(|p| p.format(Some(&unit.interner()))),
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
                ns.format(Some(&unit.interner())),
                sym.format(Some(&unit.interner())),
                dep_kind,
            );
            ns.add_depends_with(sym, dep_kind, None);
        }
    }

    // /// Bind a pattern (simple identifier or struct pattern) to a type
    // fn bind_pattern_to_type(
    //     unit: &CompileUnit<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     pattern: &HirNode<'tcx>,
    //     ty: &'tcx Symbol,
    //     type_args: &[&'tcx Symbol],
    // ) {
    //     if matches!(
    //         pattern.kind_id(),
    //         LangRust::type_identifier | LangRust::field_identifier
    //     ) {
    //         return;
    //     }

    //     if let Some(ident) = pattern.as_ident() {
    //         if let Some(sym) = ident.opt_symbol() {
    //             sym.set_type_of(ty.id());
    //             sym.add_depends(ty, Some(&[SymKind::TypeParameter, SymKind::Variable]));
    //             for arg in type_args {
    //                 sym.add_depends(arg, Some(&[SymKind::TypeParameter, SymKind::Variable]));
    //             }
    //         }
    //         return;
    //     }

    //     if let Some(field_ident) = pattern.ident_by_field(*unit, LangRust::field_name) {
    //         let field_ty = {
    //             let mut resolver = TyCtxt::new(unit, scopes);
    //             resolver
    //                 .resolve_field_type(ty, &field_ident.name)
    //                 .and_then(|(_, ty)| ty)
    //         };
    //         if let Some(field_ty) = field_ty {
    //             if let Some(subpattern) = pattern.child_by_field(*unit, LangRust::field_pattern) {
    //                 Self::bind_pattern_to_type(unit, scopes, &subpattern, field_ty, &[]);
    //             } else if let Some(sym) = field_ident.opt_symbol() {
    //                 sym.set_type_of(field_ty.id());
    //                 sym.add_depends(
    //                     field_ty,
    //                     Some(&[SymKind::TypeParameter, SymKind::Variable]),
    //                 );
    //             }
    //             return;
    //         }
    //     }

    //     if let Some(subpattern) = pattern.child_by_field(*unit, LangRust::field_pattern) {
    //         Self::bind_pattern_to_type(unit, scopes, &subpattern, ty, &[]);
    //         return;
    //     }

    //     for child in pattern.children(unit) {
    //         Self::bind_pattern_to_type(unit, scopes, &child, ty, &[]);
    //     }
    // }
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
        {
            if let Some(scope_id) = file_sym.opt_scope() {
                tracing::trace!("pushing file scope {} {:?}", file_path, scope_id);
                scopes.push_scope(scope_id);

                let file_scope = unit.get_scope(scope_id);
                self.visit_children(unit, node, scopes, file_scope, Some(file_sym));
                scopes.pop_until(depth);
                return;
            }
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
                    Some(&[SymKind::TypeParameter]),
                );
            }
        }
    }

    #[tracing::instrument(skip_all)]
    fn visit_primitive_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let ident = node.as_ident().unwrap();
        if let Some(symbol) = scopes.lookup_global(&ident.name, vec![SymKind::Primitive]) {
            ident.set_symbol(symbol);
        }
    }

    #[tracing::instrument(skip_all)]
    fn visit_type_identifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
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
                    if let Some(self_sym) = scopes.lookup_symbol(key, vec![SymKind::TypeAlias]) {
                        if let Some(struct_sym) = sn.opt_symbol() {
                            tracing::trace!(
                                "binding '{}' to struct type '{}'",
                                key,
                                struct_sym.format(Some(&unit.interner())),
                            );
                            self_sym.set_type_of(struct_sym.id());
                            // assign scope
                            if let Some(struct_scope) = struct_sym.opt_scope() {
                                self_sym.set_scope(struct_scope);
                            }
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
                        target_sym.format(Some(&unit.interner())),
                        resolved.format(Some(&unit.interner())),
                    );
                    // Update the unresolved symbol to point to the actual type
                    target_sym.set_type_of(resolved.id());
                    target_sym.set_kind(resolved.kind());
                    target_sym.add_depends(resolved, None);
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

            if let Some(trait_node) = node.child_by_field(*unit, LangRust::field_trait) {
                let mut ty_ctxt = TyCtxt::new(unit, scopes);
                if let Some(trait_sym) = ty_ctxt.resolve_type(&trait_node)
                    && let Some(target_resolved) = target_resolved
                    && let Some(target_scope) = target_resolved.opt_scope()
                    && let Some(trait_scope) = trait_sym.opt_scope()
                {
                    let target_scope = unit.get_scope(target_scope);
                    let trait_scope = unit.get_scope(trait_scope);
                    tracing::trace!(
                        "adding impl realtion: target '{}' implements trait '{}'",
                        target_resolved.format(Some(&unit.interner())),
                        trait_sym.format(Some(&unit.interner())),
                    );
                    target_scope.add_parent(trait_scope);
                    target_resolved.add_depends_with(trait_sym, DepKind::Implements, None);
                }
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
                ns.add_depends_with(enum_symbol, DepKind::Calls, Some(&[SymKind::TypeParameter]));
            } else {
                ns.add_depends_with(
                    target_symbol,
                    DepKind::Calls,
                    Some(&[SymKind::TypeParameter]),
                );
            }
        }

        // For scoped calls like Type::method(), also add depends on the Type
        if let Some(func_node) = node.child_by_field(*unit, LangRust::field_function)
            && func_node.kind_id() == LangRust::scoped_identifier
            && let Some(ns) = namespace.opt_symbol()
        {
            if let Some(path_type) = ty_ctxt.resolve_type(&func_node) {
                ns.add_depends_with(path_type, DepKind::Uses, Some(&[SymKind::TypeParameter]));
            }
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
            ns.add_depends_with(sym, DepKind::Calls, Some(&[SymKind::TypeParameter]));
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
            const_sym.add_depends(ty, None);
            if let Some(ns) = namespace.opt_symbol() {
                ns.add_depends(const_sym, Some(&[SymKind::TypeParameter]));
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
                type_sym.format(Some(&unit.interner())),
            );

            // Resolve the type that this alias points to
            if let Some(type_node) = node.child_by_field(*unit, LangRust::field_type) {
                let mut ty_ctxt = TyCtxt::new(unit, scopes);
                if let Some(resolved_type) = ty_ctxt.resolve_type(&type_node) {
                    tracing::trace!(
                        "type alias '{}' resolves to '{}'",
                        type_sym.format(Some(&unit.interner())),
                        resolved_type.format(Some(&unit.interner())),
                    );

                    type_sym.set_type_of(resolved_type.id());
                    type_sym.add_depends_with(
                        resolved_type,
                        DepKind::Alias,
                        Some(&[SymKind::TypeParameter]),
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
        {
            if let Some(ty) = TyCtxt::new(unit, scopes).resolve_type(&type_node) {
                const_sym.set_type_of(ty.id());
                const_sym.add_depends(ty, Some(&[SymKind::TypeParameter]));

                if let Some(ns) = namespace.opt_symbol() {
                    ns.add_depends(ty, Some(&[SymKind::TypeParameter]));
                }
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
                type_sym.add_depends(resolved_type, Some(&[SymKind::TypeParameter]));

                // Add namespace dependency
                if let Some(ns) = namespace.opt_symbol() {
                    ns.add_depends(resolved_type, Some(&[SymKind::TypeParameter]));
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
                ns.add_depends(resolved_type, Some(&[SymKind::TypeParameter]));
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
                    ns.add_depends(resolved_type, Some(&[SymKind::TypeParameter]));
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
                ns.add_depends(resolved_trait, Some(&[SymKind::TypeParameter]));
            }
        }
    }

    fn visit_generic_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // Generic types track both the container type and all type arguments
        // Example: Vec<Message>, Option<Config>, HashMap<K, V>
        let owner_sym = if let Some(sym) = namespace.opt_symbol() {
            sym
        } else if let Some(parent_sym) = parent
            && let Some(resolved) = unit.opt_get_symbol(parent_sym.id())
        {
            resolved
        } else {
            return;
        };

        // First, resolve the container type (Vec, Option, HashMap, etc.)
        if let Some(type_node) = node.child_by_field(*unit, LangRust::field_type) {
            let mut ty_ctxt = TyCtxt::new(unit, scopes);
            if let Some(resolved_type) = ty_ctxt.resolve_type(&type_node) {
                owner_sym.add_depends(resolved_type, Some(&[SymKind::TypeParameter]));
            }
        }

        // Second, resolve all type arguments
        if let Some(type_args_node) = node.child_by_field(*unit, LangRust::field_type_arguments) {
            let mut ty_ctxt = TyCtxt::new(unit, scopes);

            // Type arguments are children of the type_arguments node
            for arg_child in type_args_node.children(unit) {
                // Skip non-type nodes (Text, Comment, punctuation like '<', '>', ',')
                if matches!(arg_child.kind(), HirKind::Text | HirKind::Comment) {
                    continue;
                }

                // Try to resolve each type argument
                if let Some(resolved_arg_type) = ty_ctxt.resolve_type(&arg_child) {
                    owner_sym.add_depends(resolved_arg_type, Some(&[SymKind::TypeParameter]));
                }
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
                owner_sym.add_depends(resolved_type, Some(&[SymKind::TypeParameter]));
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
            // Add dependency from variant to enum
            ns.add_depends(symbol, Some(&[SymKind::TypeParameter]));

            // Handle tuple-like variants: Value(i32, String)
            // The types are in the body field
            if let Some(body_node) = node.child_by_field(*unit, LangRust::field_body) {
                // Collect all identifiers in the body (field types)
                for ident in body_node.collect_idents(unit) {
                    if let Some(ident_sym) = ident.opt_symbol() {
                        symbol.add_depends(ident_sym, Some(&[SymKind::TypeParameter]));
                        // Also add to enum for architecture tracking
                        ns.add_depends(ident_sym, Some(&[SymKind::TypeParameter]));
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
                    owner.add_depends(resolved_type, Some(&[SymKind::TypeParameter]));
                }

                // Also add from namespace (closure, etc.)
                if let Some(ns) = namespace.opt_symbol() {
                    ns.add_depends(resolved_type, Some(&[SymKind::TypeParameter]));
                }
            }
        }
    }

    // fn visit_scoped_identifier(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     // For scoped identifiers like E::V1 or E::V2, resolve the full path
    //     // If it's an enum variant, add dependency on the parent enum via type_of
    //     if let Some(owner) = namespace.opt_symbol()
    //         && let Some(sym) =
    //             TyCtxt::new(unit, scopes).resolve_scoped_identifier_type(node, None)
    //         && sym.kind() == SymKind::EnumVariant
    //         && let Some(parent_enum_id) = sym.type_of()
    //         && let Some(parent_enum) = unit.opt_get_symbol(parent_enum_id)
    //     {
    //         owner.add_depends(parent_enum, Some(&[SymKind::TypeParameter]));
    //     }
    //     self.visit_children(unit, node, scopes, namespace, parent);
    // }

    // fn visit_let_declaration(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     self.visit_children(unit, node, scopes, namespace, parent);

    //     let (ty, type_args) =
    //         if let Some(type_node) = node.child_by_field(*unit, LangRust::field_type) {
    //             let mut resolver = TyCtxt::new(unit, scopes);
    //             let ty = resolver.resolve_type_node(&type_node);
    //             let type_args = resolver.collect_type_argument_symbols(&type_node);
    //             (ty, type_args)
    //         } else if let Some(value_node) = node.child_by_field(*unit, LangRust::field_value) {
    //             let ty = TyCtxt::new(unit, scopes).infer_type_from_expr(&value_node);
    //             (ty, Vec::new())
    //         } else {
    //             (None, Vec::new())
    //         };

    //     if let Some(ty) = ty {
    //         if let Some(pattern) = node.child_by_field(*unit, LangRust::field_pattern) {
    //             Self::bind_pattern_to_type(unit, scopes, &pattern, ty, &type_args);
    //         }

    //         if let Some(ns) = namespace.opt_symbol() {
    //             ns.add_depends(ty, Some(&[SymKind::TypeParameter, SymKind::Variable]));
    //             // Also add dependencies on type arguments to the parent
    //             for arg_sym in &type_args {
    //                 ns.add_depends(arg_sym, Some(&[SymKind::TypeParameter, SymKind::Variable]));
    //             }
    //         }
    //     }
    // }

    // fn visit_struct_expression(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     self.visit_children(unit, node, scopes, namespace, parent);

    //     if let Some(name_node) = node
    //         .child_by_field(*unit, LangRust::field_name)
    //         .or_else(|| node.child_by_field(*unit, LangRust::field_type))
    //         && let Some(ty) = TyCtxt::new(unit, scopes).infer_type_from_expr(&name_node)
    //         && let Some(caller) = parent
    //     {
    //         caller.add_depends(ty, None);
    //     }
    // }
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

#[cfg(test)]
mod tests {
    use crate::token::LangRust;

    use llmcc_core::context::CompileCtxt;
    use llmcc_core::ir_builder::{IrBuildOption, build_llmcc_ir};
    use llmcc_core::symbol::{DepKind, SymKind};
    use llmcc_resolver::{ResolverOption, bind_symbols_with, collect_symbols_with};

    fn with_compiled_unit<F>(sources: &[&str], check: F)
    where
        F: for<'a> FnOnce(&'a CompileCtxt<'a>),
    {
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
        let globals = collect_symbols_with::<LangRust>(&cc, &resolver_option);
        bind_symbols_with::<LangRust>(&cc, &globals, &resolver_option);
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

    fn assert_depends<'a>(
        cc: &'a CompileCtxt<'a>,
        from_name: &str,
        from_kind: SymKind,
        to_name: &str,
        to_kind: SymKind,
        dep_kind: Option<DepKind>,
    ) {
        // Find the symbols by name and kind - use the ones from get_all_symbols() which are from the arena
        let from_sym = cc
            .get_all_symbols()
            .iter()
            .find(|sym| {
                let name_key = cc.interner.intern(from_name);
                sym.name == name_key && sym.kind() == from_kind
            })
            .copied()
            .expect(&format!(
                "symbol {} with kind {:?} not found",
                from_name, from_kind
            ));

        let to_sym = cc
            .get_all_symbols()
            .iter()
            .find(|sym| {
                let name_key = cc.interner.intern(to_name);
                sym.name == name_key && sym.kind() == to_kind
            })
            .copied()
            .expect(&format!(
                "symbol {} with kind {:?} not found",
                to_name, to_kind
            ));

        let from_id = from_sym.id();
        let to_id = to_sym.id();

        let has_dep = if let Some(kind) = dep_kind {
            from_sym
                .depends
                .read()
                .iter()
                .any(|(dep_id, dep_k)| *dep_id == to_id && *dep_k == kind)
        } else {
            from_sym
                .depends
                .read()
                .iter()
                .any(|(dep_id, _)| *dep_id == to_id)
        };

        assert!(
            has_dep,
            "'{}' ({:?}) should depend on '{}' ({:?}){}",
            from_name,
            from_kind,
            to_name,
            to_kind,
            dep_kind
                .map(|k| format!(" with kind {:?}", k))
                .unwrap_or_default()
        );
    }

    fn assert_exists<'a>(cc: &'a CompileCtxt<'a>, name: &str, kind: SymKind) {
        let name_key = cc.interner.intern(name);
        let all_symbols = cc.get_all_symbols();
        let symbol = all_symbols
            .iter()
            .find(|sym| sym.name == name_key && sym.kind() == kind)
            .expect(&format!("symbol {} with kind {:?} not found", name, kind));
        assert!(symbol.id().0 > 0, "symbol should have a valid id");
    }

    #[test]
    #[serial_test::serial]
    fn test_visit_source_file() {
        let source = r#"
            fn main() {}
        "#;

        with_compiled_unit(&[source], |cc| {
            let all_symbols = cc.get_all_symbols();
            assert!(!all_symbols.is_empty());
        });
    }

    #[test]
    #[serial_test::serial]
    fn test_visit_mod_item() {
        let source = r#"
            mod utils {}
        "#;

        with_compiled_unit(&[source], |cc| {
            assert_depends(
                cc,
                "source_0",
                SymKind::File,
                "utils",
                SymKind::Namespace,
                None,
            );
        });
    }

    #[test]
    #[serial_test::serial]
    fn test_visit_function_item() {
        let source = r#"
            struct Option<T> {}

            fn get_value() -> Option<i32> {
                Some(42)
            }

            struct User {
                name: String,
            }

            impl User {
                fn new(name: String) -> User {
                    User { name }
                }

                fn foo() {
                    println!("foo");
                }

                fn display(&self) {
                    println!("User: {}", self.name);
                    Self::foo();
                }
            }

            fn main() {
                let user = User::new(String::from("Alice"));
                user.display();
            }
        "#;

        with_compiled_unit(&[source], |cc| {
            // Test return type dependencies for standalone function
            assert_depends(
                cc,
                "get_value",
                SymKind::Function,
                "Option",
                SymKind::Struct,
                Some(DepKind::ReturnType),
            );

            // Test return type in impl block (explicit type instead of Self)
            assert_depends(
                cc,
                "new",
                SymKind::Function,
                "User",
                SymKind::Struct,
                Some(DepKind::ReturnType),
            );

            assert_depends(
                cc,
                "display",
                SymKind::Function,
                "foo",
                SymKind::Function,
                Some(DepKind::Uses),
            );
        });
    }

    #[test]
    #[serial_test::serial]
    fn test_visit_impl_item() {
        let source = r#"
            trait Printable {
                fn print(&self);
            }

            struct Container<T, U, V>(T);
            struct Inner;
            struct Foo;
            struct Outer<T>;

            impl Container<Inner, Foo, Outer<Foo>> {
                fn new(value: Inner) -> Container<Inner, Foo, Outer<Foo>> {
                    Container(value)
                }
            }

            impl Printable for Container<Inner, Foo, Outer<Foo>> {
                fn print(&self) {
                    println!("Printing Inner container.");
                }
            }
        "#;

        with_compiled_unit(&[source], |cc| {
            assert_depends(
                cc,
                "Container",
                SymKind::Struct,
                "new",
                SymKind::Function,
                Some(DepKind::Uses),
            );

            assert_depends(
                cc,
                "Container",
                SymKind::Struct,
                "Outer",
                SymKind::Struct,
                Some(DepKind::Uses),
            );
        });
    }

    #[test]
    #[serial_test::serial]
    fn test_visit_struct_item() {
        let source = r#"
            struct User {
                name: String,
            }
        "#;

        with_compiled_unit(&[source], |cc| {
            assert_exists(cc, "User", SymKind::Struct);
        });
    }

    #[test]
    #[serial_test::serial]
    fn test_visit_enum_item() {
        let source = r#"
            enum Color {
                Red,
                Green,
                Blue,
            }
        "#;

        with_compiled_unit(&[source], |cc| {
            assert_exists(cc, "Color", SymKind::Enum);
        });
    }

    #[test]
    #[serial_test::serial]
    fn test_visit_trait_item() {
        let source = r#"
            trait Display {
                fn display(&self);
            }

            trait Clone {
                fn clone(&self) -> Self;
            }

            trait Iterator {
                type Item;
                fn next(&mut self) -> Option<Self::Item>;
            }

            trait Sized {}

            trait FromIterator<T>: Sized + Clone {
                fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self;
            }
        "#;

        with_compiled_unit(&[source], |cc| {
            // Test Display trait with method
            assert_depends(
                cc,
                "Display",
                SymKind::Trait,
                "display",
                SymKind::Function,
                Some(DepKind::Uses),
            );

            // Test Clone trait
            assert_depends(
                cc,
                "Clone",
                SymKind::Trait,
                "clone",
                SymKind::Function,
                Some(DepKind::Uses),
            );

            // Test FromIterator trait with bound
            assert_depends(
                cc,
                "FromIterator",
                SymKind::Trait,
                "Sized",
                SymKind::Trait,
                Some(DepKind::TypeBound),
            );

            assert_depends(
                cc,
                "FromIterator",
                SymKind::Trait,
                "Clone",
                SymKind::Trait,
                Some(DepKind::TypeBound),
            );
        });
    }

    #[test]
    #[serial_test::serial]
    fn test_visit_macro_definition() {
        let source = r#"
            macro_rules! hello {
                () => {
                    println!("Hello!");
                };
            }
        "#;

        with_compiled_unit(&[source], |cc| {
            assert_exists(cc, "hello", SymKind::Macro);
        });
    }

    #[test]
    #[serial_test::serial]
    fn test_visit_macro_invocation() {
        let source = r#"
            macro_rules! hello {
                () => {
                    println!("Hello!");
                };
            }

            fn main() {
                hello!();
            }
        "#;

        with_compiled_unit(&[source], |cc| {
            assert_depends(
                cc,
                "main",
                SymKind::Function,
                "hello",
                SymKind::Macro,
                Some(DepKind::Calls),
            );
        });
    }

    #[test]
    #[serial_test::serial]
    fn test_visit_function_signature_item() {
        let source = r#"
            fn add(a: i32, b: i32) -> i32;
        "#;

        with_compiled_unit(&[source], |cc| {
            assert_exists(cc, "add", SymKind::Function);
        });
    }

    #[test]
    #[serial_test::serial]
    fn test_visit_type_item() {
        let source = r#"
            trait Printable {
                fn print(&self);
            }

            trait Serializable {
                fn serialize(&self) -> String;
            }

            struct Data<T> {
                value: T,
            }

            type PrintableData<T> = Data<T> where T: Printable;
            type SerializableCollection<T> = Data<T> where T: Serializable + Printable;
        "#;

        with_compiled_unit(&[source], |cc| {
            // Test type alias with where clause
            assert_exists(cc, "PrintableData", SymKind::TypeAlias);
            assert_depends(
                cc,
                "PrintableData",
                SymKind::TypeAlias,
                "Data",
                SymKind::Struct,
                Some(DepKind::Alias),
            );

            assert_depends(
                cc,
                "PrintableData",
                SymKind::TypeAlias,
                "Printable",
                SymKind::Trait,
                Some(DepKind::Uses),
            );

            // Test type alias with multiple where clause bounds
            assert_exists(cc, "SerializableCollection", SymKind::TypeAlias);
            assert_depends(
                cc,
                "SerializableCollection",
                SymKind::TypeAlias,
                "Data",
                SymKind::Struct,
                Some(DepKind::Alias),
            );

            assert_depends(
                cc,
                "SerializableCollection",
                SymKind::TypeAlias,
                "Serializable",
                SymKind::Trait,
                Some(DepKind::Uses),
            );
            assert_depends(
                cc,
                "SerializableCollection",
                SymKind::TypeAlias,
                "Printable",
                SymKind::Trait,
                Some(DepKind::Uses),
            );
        });
    }

    #[test]
    #[serial_test::serial]
    fn test_visit_type_parameter() {
        let source = r#"
            pub trait Trait1 {}
            pub trait Trait2 {}

            // Test simple bound
            pub fn simple<T: Trait1>() {}

            // Test multiple bounds
            pub fn multiple<U: Trait1 + Trait2>() {}

            // Test struct with type parameter bounds
            pub struct GenericStruct<V: Trait1> {
                val: V,
            }

            // Test trait with type parameter bounds
            pub trait GenericTrait<W: Trait2> {
                fn method(&self);
            }
            "#;
        with_compiled_unit(&[source], |cc| {
            // Test simple bound: T should have TypeBound dependency on Trait1
            assert_depends(
                cc,
                "simple",
                SymKind::Function,
                "Trait1",
                SymKind::Trait,
                Some(DepKind::TypeBound),
            );

            // Test multiple bounds: U should have TypeBound dependencies on both traits
            assert_depends(
                cc,
                "multiple",
                SymKind::Function,
                "Trait1",
                SymKind::Trait,
                Some(DepKind::TypeBound),
            );
            assert_depends(
                cc,
                "multiple",
                SymKind::Function,
                "Trait2",
                SymKind::Trait,
                Some(DepKind::TypeBound),
            );

            // Test struct with type parameter bounds
            assert_depends(
                cc,
                "GenericStruct",
                SymKind::Struct,
                "Trait1",
                SymKind::Trait,
                Some(DepKind::TypeBound),
            );

            // Test trait with type parameter bounds
            assert_depends(
                cc,
                "GenericTrait",
                SymKind::Trait,
                "Trait2",
                SymKind::Trait,
                Some(DepKind::TypeBound),
            );
        });
    }

    #[test]
    #[serial_test::serial]
    fn test_visit_const_parameter() {
        let source = r#"
            // Custom complex types for const parameters
            pub struct Config;
            pub struct Settings;
            pub enum Mode { Fast, Slow }

            // Test const parameter with custom type
            pub fn process_config<const CFG: Config>() {}

            // Test const parameter in struct with custom type
            pub struct Container<const S: Settings> {
                data: u32,
            }

            // Test const parameter in trait with custom type
            pub trait Processor<const M: Mode> {
                fn process(&self);
            }

            // Test multiple const parameters with complex types
            pub struct Complex<const A: Settings, const B: Config> {
                value: i32,
            }
            "#;
        with_compiled_unit(&[source], |cc| {
            // Test const parameter in function: function should depend on Config
            assert_depends(
                cc,
                "process_config",
                SymKind::Function,
                "Config",
                SymKind::Struct,
                None,
            );

            // Test const parameter in struct: struct should depend on Settings
            assert_depends(
                cc,
                "Container",
                SymKind::Struct,
                "Settings",
                SymKind::Struct,
                None,
            );

            // Test const parameter in trait: trait should depend on Mode
            assert_depends(
                cc,
                "Processor",
                SymKind::Trait,
                "Mode",
                SymKind::Enum,
                None,
            );

            // Test multiple const parameters: Complex should depend on both Settings and Config
            assert_depends(
                cc,
                "Complex",
                SymKind::Struct,
                "Settings",
                SymKind::Struct,
                None,
            );

            assert_depends(
                cc,
                "Complex",
                SymKind::Struct,
                "Config",
                SymKind::Struct,
                None,
            );
        });
    }

    #[test]
    #[serial_test::serial]
    fn test_visit_associated_type() {
        let source = r#"
            // Custom types
            pub struct DataType;
            pub struct ConfigType;
            pub struct ResultType;

            // Test associated type with default
            pub trait Processor {
                type OutputType = DataType;
                type InputType = ConfigType;
            }

            // Test associated type with complex default
            pub trait Handler {
                type ResponseType = ResultType;
            }
            "#;
        with_compiled_unit(&[source], |cc| {
            // Associated types with defaults should have Alias dependencies to their types

            // Test OutputType associated type depends on DataType struct
            assert_depends(
                cc,
                "OutputType",
                SymKind::TypeAlias,
                "DataType",
                SymKind::Struct,
                Some(DepKind::Alias),
            );

            // Test InputType associated type depends on ConfigType struct
            assert_depends(
                cc,
                "InputType",
                SymKind::TypeAlias,
                "ConfigType",
                SymKind::Struct,
                Some(DepKind::Alias),
            );

            // Test ResponseType associated type depends on ResultType struct
            assert_depends(
                cc,
                "ResponseType",
                SymKind::TypeAlias,
                "ResultType",
                SymKind::Struct,
                Some(DepKind::Alias),
            );
        });
    }

    #[test]
    #[serial_test::serial]
    fn test_visit_where_predicate() {
        let source = r#"
            // Traits for bounds
            pub trait Clone {}
            pub trait Display {}
            pub trait Debug {}
            pub trait Sized {}

            // Test where predicate on function
            pub fn process<T>() where T: Clone + Display {}

            // Test where predicate on struct
            pub struct Container<T> where T: Debug {
                value: T,
            }

            // Test where predicate on trait
            pub trait Iterable<T> where T: Clone {
                fn iter(&self) -> T;
            }

            // Test where predicate with multiple bounds
            pub fn complex<U>() where U: Clone + Display + Debug {}
            "#;
        with_compiled_unit(&[source], |cc| {
            // Test where predicate on function: process should depend on Clone and Display
            assert_depends(
                cc,
                "process",
                SymKind::Function,
                "Clone",
                SymKind::Trait,
                Some(DepKind::TypeBound),
            );

            assert_depends(
                cc,
                "process",
                SymKind::Function,
                "Display",
                SymKind::Trait,
                Some(DepKind::TypeBound),
            );

            // Test where predicate on struct: Container should depend on Debug
            assert_depends(
                cc,
                "Container",
                SymKind::Struct,
                "Debug",
                SymKind::Trait,
                Some(DepKind::TypeBound),
            );

            // Test where predicate on trait: Iterable should depend on Clone
            assert_depends(
                cc,
                "Iterable",
                SymKind::Trait,
                "Clone",
                SymKind::Trait,
                Some(DepKind::TypeBound),
            );

            // Test complex where predicate with multiple bounds
            assert_depends(
                cc,
                "complex",
                SymKind::Function,
                "Clone",
                SymKind::Trait,
                Some(DepKind::TypeBound),
            );

            assert_depends(
                cc,
                "complex",
                SymKind::Function,
                "Display",
                SymKind::Trait,
                Some(DepKind::TypeBound),
            );

            assert_depends(
                cc,
                "complex",
                SymKind::Function,
                "Debug",
                SymKind::Trait,
                Some(DepKind::TypeBound),
            );
        });
    }

    #[test]
    #[serial_test::serial]
    fn test_visit_array_type() {
        let source = r#"
            // Custom struct for array elements
            pub struct Data;
            pub struct Config;
            pub struct Message;

            // Test simple array type
            pub fn process_array(arr: [Data; 10]) {}

            // Test array type in struct
            pub struct Buffer {
                items: [Config; 5],
            }

            // Test array type in function return
            pub fn create_messages() -> [Message; 3] {
                [Message, Message, Message]
            }

            // Test nested array type
            pub fn process_matrix(matrix: [[Data; 4]; 3]) {}
            "#;
        with_compiled_unit(&[source], |cc| {
            // Test array element type dependency in function parameter
            assert_depends(
                cc,
                "process_array",
                SymKind::Function,
                "Data",
                SymKind::Struct,
                None,
            );

            // Test array element type dependency in struct field
            assert_depends(
                cc,
                "Buffer",
                SymKind::Struct,
                "Config",
                SymKind::Struct,
                None,
            );

            // Test array element type dependency in return type
            assert_depends(
                cc,
                "create_messages",
                SymKind::Function,
                "Message",
                SymKind::Struct,
                None,
            );

            // Test nested array element type dependency
            assert_depends(
                cc,
                "process_matrix",
                SymKind::Function,
                "Data",
                SymKind::Struct,
                None,
            );
        });
    }

    #[test]
    #[serial_test::serial]
    fn test_visit_tuple_type() {
        let source = r#"
            pub struct Config {}
            pub struct Message {}
            pub struct Data {}
            pub struct Response {}

            // Test function with tuple parameter
            pub fn handle_pair(pair: (Config, Message)) {}

            // Test struct with tuple field
            pub struct Container {
                items: (Data, Response, Config),
            }

            // Test function with tuple return type
            pub fn create_triple() -> (Message, Data, Config) {
                todo!()
            }

            // Test nested tuple types
            pub fn nested_tuples(data: ((Config, Message), (Data, Response))) {}
            "#;
        with_compiled_unit(&[source], |cc| {
            // Test function parameter tuple: handle_pair depends on Config and Message
            assert_depends(
                cc,
                "handle_pair",
                SymKind::Function,
                "Config",
                SymKind::Struct,
                None,
            );
            assert_depends(
                cc,
                "handle_pair",
                SymKind::Function,
                "Message",
                SymKind::Struct,
                None,
            );

            // Test struct field tuple: Container depends on Data, Response, Config
            assert_depends(
                cc,
                "Container",
                SymKind::Struct,
                "Data",
                SymKind::Struct,
                None,
            );
            assert_depends(
                cc,
                "Container",
                SymKind::Struct,
                "Response",
                SymKind::Struct,
                None,
            );
            assert_depends(
                cc,
                "Container",
                SymKind::Struct,
                "Config",
                SymKind::Struct,
                None,
            );

            // Test function return type tuple: create_triple depends on Message, Data, Config
            assert_depends(
                cc,
                "create_triple",
                SymKind::Function,
                "Message",
                SymKind::Struct,
                None,
            );
            assert_depends(
                cc,
                "create_triple",
                SymKind::Function,
                "Data",
                SymKind::Struct,
                None,
            );
            assert_depends(
                cc,
                "create_triple",
                SymKind::Function,
                "Config",
                SymKind::Struct,
                None,
            );

            // Test nested tuple: nested_tuples depends on all types in nested tuples
            assert_depends(
                cc,
                "nested_tuples",
                SymKind::Function,
                "Config",
                SymKind::Struct,
                None,
            );
            assert_depends(
                cc,
                "nested_tuples",
                SymKind::Function,
                "Message",
                SymKind::Struct,
                None,
            );
            assert_depends(
                cc,
                "nested_tuples",
                SymKind::Function,
                "Data",
                SymKind::Struct,
                None,
            );
            assert_depends(
                cc,
                "nested_tuples",
                SymKind::Function,
                "Response",
                SymKind::Struct,
                None,
            );
        });
    }

    #[test]
    #[serial_test::serial]
    fn test_visit_abstract_type() {
        let source = r#"
            pub trait Iterator {}
            pub trait Display {}
            pub trait Clone {}
            pub trait Send {}
            pub trait Sync {}

            // Test function with impl Trait return type (simple)
            pub fn get_iterator() -> impl Iterator {
                todo!()
            }

            // Test function with impl Trait return type (generic trait with args)
            pub fn get_display() -> impl Display {
                todo!()
            }

            // Test struct impl block method with impl Trait
            pub struct Worker;
            impl Worker {
                pub fn create_clone(&self) -> impl Clone {
                    todo!()
                }
            }

            // Test trait method with impl Trait
            pub trait Producer {
                fn produce(&self) -> impl Display;
            }

            // Test multiple impl Trait in different functions
            pub fn sync_handler() -> impl Sync {
                todo!()
            }

            // Test impl Trait with single bound using + operator
            pub fn bounded_iterator() -> impl Iterator + Send {
                todo!()
            }
            "#;
        with_compiled_unit(&[source], |cc| {
            // Test 1: get_iterator should depend on Iterator trait
            assert_depends(
                cc,
                "get_iterator",
                SymKind::Function,
                "Iterator",
                SymKind::Trait,
                None,
            );

            // Test 2: get_display should depend on Display trait
            assert_depends(
                cc,
                "get_display",
                SymKind::Function,
                "Display",
                SymKind::Trait,
                None,
            );

            // Test 3: Worker::create_clone should depend on Clone trait
            assert_depends(
                cc,
                "create_clone",
                SymKind::Function,
                "Clone",
                SymKind::Trait,
                None,
            );

            // Test 4: sync_handler should depend on Sync trait
            assert_depends(
                cc,
                "sync_handler",
                SymKind::Function,
                "Sync",
                SymKind::Trait,
                None,
            );

            // Test 5: bounded_iterator should depend on Iterator (primary trait)
            // The + operator creates a bounded_type node, so Iterator is the first trait
            assert_depends(
                cc,
                "bounded_iterator",
                SymKind::Function,
                "Iterator",
                SymKind::Trait,
                None,
            );
        });
    }

    #[test]
    #[serial_test::serial]
    fn test_visit_enum_variant() {
        let source = r#"
            pub struct Point {}
            pub struct Data {}
            pub struct Message {}

            // Test enum with tuple-like variants
            pub enum Result {
                Ok(Point),
                Error(Data),
            }

            // Test enum with struct-like variants
            pub enum Response {
                Success { value: Message },
                Failed { reason: Data },
            }

            // Test enum with unit and data variants mixed
            pub enum Status {
                Running,
                Stopped(Point),
                Error(Data, Message),
            }

            // Test function that uses enum variants
            pub fn create_result() -> Result {
                todo!()
            }
            "#;
        with_compiled_unit(&[source], |cc| {
            // Test 1: Result enum should depend on Point (from Ok variant)
            assert_depends(
                cc,
                "Result",
                SymKind::Enum,
                "Point",
                SymKind::Struct,
                None,
            );

            // Test 2: Result enum should depend on Data (from Error variant)
            assert_depends(
                cc,
                "Result",
                SymKind::Enum,
                "Data",
                SymKind::Struct,
                None,
            );

            // Test 3: Response enum should depend on Message (from Success field)
            assert_depends(
                cc,
                "Response",
                SymKind::Enum,
                "Message",
                SymKind::Struct,
                None,
            );

            // Test 4: Response enum should depend on Data (from Failed field)
            assert_depends(
                cc,
                "Response",
                SymKind::Enum,
                "Data",
                SymKind::Struct,
                None,
            );

            // Test 5: Status enum should depend on Point
            assert_depends(
                cc,
                "Status",
                SymKind::Enum,
                "Point",
                SymKind::Struct,
                None,
            );

            // Test 6: Status enum should depend on Data
            assert_depends(
                cc,
                "Status",
                SymKind::Enum,
                "Data",
                SymKind::Struct,
                None,
            );

            // Test 7: Status enum should depend on Message
            assert_depends(
                cc,
                "Status",
                SymKind::Enum,
                "Message",
                SymKind::Struct,
                None,
            );
        });
    }

    #[test]
    #[serial_test::serial]
    fn test_visit_parameter() {
        let source = r#"
            pub struct Point {}
            pub struct Config {}
            pub struct Data {}

            // Test function with single parameter
            pub fn process_point(pt: Point) {}

            // Test function with multiple parameters
            pub fn combined(cfg: Config, data: Data) {}

            // Test function with reference parameter
            pub fn read_data(d: &Data) {}

            // Test method with self and parameters
            pub struct Handler;
            impl Handler {
                pub fn handle(&self, pt: Point) {}

                pub fn process(&mut self, cfg: Config) {}
            }

            // Test trait method with parameters
            pub trait Processor {
                fn process(&self, data: Data);
                fn configure(&self, cfg: Config);
            }
            "#;
        with_compiled_unit(&[source], |cc| {
            // Test 1: process_point depends on Point (parameter type)
            assert_depends(
                cc,
                "process_point",
                SymKind::Function,
                "Point",
                SymKind::Struct,
                None,
            );

            // Test 2: combined depends on Config
            assert_depends(
                cc,
                "combined",
                SymKind::Function,
                "Config",
                SymKind::Struct,
                None,
            );

            // Test 3: combined depends on Data
            assert_depends(
                cc,
                "combined",
                SymKind::Function,
                "Data",
                SymKind::Struct,
                None,
            );

            // Test 4: read_data depends on Data (even with &)
            assert_depends(
                cc,
                "read_data",
                SymKind::Function,
                "Data",
                SymKind::Struct,
                None,
            );

            // Test 5: Handler::handle depends on Point (from method parameter)
            assert_depends(
                cc,
                "handle",
                SymKind::Function,
                "Point",
                SymKind::Struct,
                None,
            );

            // Test 6: Handler::process depends on Config
            assert_depends(
                cc,
                "process",
                SymKind::Function,
                "Config",
                SymKind::Struct,
                None,
            );
        });
    }

    #[test]
    #[serial_test::serial]
    fn test_visit_field_declaration() {
        let source = r#"
            pub struct Point {}
            pub struct Config {}
            pub struct Data {}
            pub struct Message {}
            pub struct Request {}
            pub struct Response {}

            // Test struct with single field
            pub struct Container {
                item: Point,
            }

            // Test struct with multiple fields
            pub struct Package {
                config: Config,
                data: Data,
            }

            // Test struct with simple field
            pub struct Handler {
                config: Config,
            }

            // Test struct with Vec<T> generic field
            pub struct MessageQueue {
                messages: Vec<Message>,
            }

            // Test struct with Option<T> generic field
            pub struct OptionalConfig {
                cfg: Option<Config>,
            }

            // Test struct with multiple generic fields
            pub struct Pipeline {
                requests: Vec<Request>,
                responses: Option<Response>,
            }

            // Test struct with nested generic types
            pub struct ComplexStorage {
                items: Vec<Vec<Data>>,
            }

            // Test function returning struct type
            pub fn create_package() -> Package {
                todo!()
            }
            "#;
        with_compiled_unit(&[source], |cc| {
            // Test 1: Container struct depends on Point (from field)
            assert_depends(
                cc,
                "Container",
                SymKind::Struct,
                "Point",
                SymKind::Struct,
                None,
            );

            // Test 2: Package struct depends on Config
            assert_depends(
                cc,
                "Package",
                SymKind::Struct,
                "Config",
                SymKind::Struct,
                None,
            );

            // Test 3: Package struct depends on Data
            assert_depends(
                cc,
                "Package",
                SymKind::Struct,
                "Data",
                SymKind::Struct,
                None,
            );

            // Test 4: Handler struct depends on Config
            assert_depends(
                cc,
                "Handler",
                SymKind::Struct,
                "Config",
                SymKind::Struct,
                None,
            );

            // Test 5: MessageQueue struct depends on Message (generic type arg)
            assert_depends(
                cc,
                "MessageQueue",
                SymKind::Struct,
                "Message",
                SymKind::Struct,
                None,
            );

            // Test 6: OptionalConfig struct depends on Config (generic type arg in Option)
            assert_depends(
                cc,
                "OptionalConfig",
                SymKind::Struct,
                "Config",
                SymKind::Struct,
                None,
            );

            // Test 7: Pipeline struct depends on Request
            assert_depends(
                cc,
                "Pipeline",
                SymKind::Struct,
                "Request",
                SymKind::Struct,
                None,
            );

            // Test 8: Pipeline struct depends on Response
            assert_depends(
                cc,
                "Pipeline",
                SymKind::Struct,
                "Response",
                SymKind::Struct,
                None,
            );

            // Test 9: ComplexStorage struct depends on Data (nested Vec<Vec<>>)
            assert_depends(
                cc,
                "ComplexStorage",
                SymKind::Struct,
                "Data",
                SymKind::Struct,
                None,
            );
        });
    }
}
