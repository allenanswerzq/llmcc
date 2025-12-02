use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirNode, HirScope};
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{DepKind, SymKind, Symbol};
use llmcc_resolver::{BinderScopes, ResolverOption};

use crate::token::AstVisitorRust;
use crate::token::LangRust;
use crate::ty::TyCtxt;
use crate::util::{parse_crate_name, parse_file_name, parse_module_name};

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

    // fn add_const_dependency(
    //     unit: &CompileUnit<'tcx>,
    //     scopes: &BinderScopes<'tcx>,
    //     node: &HirNode<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    // ) {
    //     if let Some(ident) = node.find_identifier(*unit)
    //         && let Some(owner) = namespace.opt_symbol()
    //         && let Some(sym) = ident
    //             .opt_symbol()
    //             .or_else(|| scopes.lookup_symbol(&ident.name))
    //         && matches!(
    //             sym.kind(),
    //             SymKind::Const | SymKind::Static | SymKind::EnumVariant
    //         )
    //     {
    //         owner.add_depends(sym, Some(&[SymKind::TypeParameter]));
    //     }
    // }

    // fn add_type_dependencies(owner: &Symbol, ty: Option<&Symbol>, args: &[&Symbol]) {
    //     Self::add_type_dependencies_with_kind(owner, ty, args, DepKind::Uses);
    // }

    // fn add_type_bound_dependencies(owner: &Symbol, ty: Option<&Symbol>, args: &[&Symbol]) {
    //     Self::add_type_dependencies_with_kind(owner, ty, args, DepKind::TypeBound);
    // }

    // fn add_type_dependencies_with_kind(
    //     owner: &Symbol,
    //     ty: Option<&Symbol>,
    //     args: &[&Symbol],
    //     dep_kind: DepKind,
    // ) {
    //     if let Some(symbol) = ty {
    //         owner.add_depends_with(symbol, dep_kind, Some(&[SymKind::TypeParameter]));
    //     }
    //     for arg in args {
    //         owner.add_depends_with(arg, dep_kind, Some(&[SymKind::TypeParameter]));
    //     }
    // }

    // fn add_type_dependencies_with(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     owner: &Symbol,
    // ) {
    //     let mut resolver = TyCtxt::new(unit, scopes);
    //     let (ty, args) = resolver.resolve_type_with_args(node);
    //     Self::add_type_dependencies(owner, ty, &args);
    // }

    // fn add_type_bound_with(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     owner: &Symbol,
    // ) {
    //     let mut resolver = TyCtxt::new(unit, scopes);
    //     let (ty, args) = resolver.resolve_type_with_args(node);
    //     Self::add_type_bound_dependencies(owner, ty, &args);
    // }

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

        // namespace owner to curent relationship
        if let Some(ns) = namespace.opt_symbol()
            && let Some(sym) = sn.opt_symbol()
        {
            tracing::trace!(
                "adding depends from namespace '{}' (id={}, ptr={:p}) to symbol '{}' (id={}, ptr={:p})",
                unit.resolve_name_or(ns.name, "<unnamed>"),
                ns.id().0,
                ns as *const _,
                unit.resolve_name_or(sym.name, "<unnamed>"),
                sym.id().0,
                sym as *const _,
            );
            ns.add_depends(sym, None);
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

    //     if let Some(field_ident) = pattern.child_identifier_by_field(*unit, LangRust::field_name) {
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

    // fn collect_nested_call_deps(
    //     unit: &CompileUnit<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     node: &HirNode<'tcx>,
    //     outer_target: &'tcx Symbol,
    //     parent: Option<&Symbol>,
    // ) {
    //     if node.kind_id() == LangRust::call_expression
    //         && let Some(inner_sym) =
    //             TyCtxt::new(unit, scopes).resolve_call_target(node, parent)
    //     {
    //         outer_target.add_depends(inner_sym, Some(&[SymKind::TypeParameter]));
    //     }
    //     for child in node.children(unit) {
    //         Self::collect_nested_call_deps(unit, scopes, &child, outer_target, parent);
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

    fn visit_mod_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        tracing::trace!("visit_mod_item");
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
        tracing::trace!("visit_function_item");
        let sn = node.as_scope().unwrap();
        self.visit_scoped_named(unit, node, sn, scopes, namespace, parent);

        // At this point, all return type node should already be bound
        let ret_ident = node.child_identifier_by_field(*unit, LangRust::field_return_type);

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

            let ret_full_node = node.child_by_field(*unit, LangRust::field_return_type);
            if let Some(ref ret_node) = ret_full_node {
                let type_args = TyCtxt::new(unit, scopes).collect_type_args(ret_node);
                for arg_sym in type_args {
                    tracing::trace!(
                        "adding return type argument depend from function '{}' to type argument '{}'",
                        unit.resolve_name_or(fn_sym.name, "<unnamed>"),
                        unit.resolve_name_or(arg_sym.name, "<unnamed>"),
                    );
                    fn_sym.add_depends_with(
                        arg_sym,
                        DepKind::ReturnType,
                        Some(&[SymKind::TypeParameter]),
                    );
                }
            }
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
        tracing::trace!("binding struct_item");
        let sn = node.as_scope().unwrap();
        self.visit_scoped_named(unit, node, sn, scopes, namespace, parent);
    }

    // fn visit_enum_item(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     let sn = node.as_scope().unwrap();
    //     self.visit_scoped_named(unit, node, sn, scopes, namespace, parent);
    // }

    // fn visit_trait_item(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     let sn = node.as_scope().unwrap();
    //     self.visit_scoped_named(unit, node, sn, scopes, namespace, parent);

    //     // Process trait bounds (supertrait dependencies)
    //     if let Some(trait_sym) = sn.opt_symbol()
    //         && let Some(bounds_node) = node.child_by_field(*unit, LangRust::field_bounds)
    //     {
    //         self.add_type_dependencies_with(unit, &bounds_node, scopes, trait_sym);
    //     }
    // }

    // fn visit_impl_item(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     let sn = node.as_scope().unwrap();
    //     self.visit_scoped_named(unit, node, sn, scopes, namespace, parent);

    //     if let Some(type_ident) = node.child_identifier_by_field(*unit, LangRust::field_type)
    //         && let Some(target_sym) = type_ident.opt_symbol()
    //     {
    //         // Resolve to get the actual struct/type symbol for cross-module cases
    //         let resolved_target =
    //             if let Some(type_node) = node.child_by_field(*unit, LangRust::field_type) {
    //                 let mut resolver = TyCtxt::new(unit, scopes);
    //                 resolver.resolve_type_node(&type_node)
    //             } else {
    //                 None
    //             };

    //         // Use resolved target if available, otherwise fall back to local symbol
    //         let actual_target = resolved_target.unwrap_or(target_sym);

    //         if target_sym.kind() == SymKind::UnresolvedType {
    //             // Resolve the type for the impl type now
    //             if let Some(resolved) = resolved_target
    //                 && resolved.id() != target_sym.id()
    //                 && !matches!(resolved.kind(), SymKind::UnresolvedType)
    //             {
    //                 // Update the unresolved symbol to point to the actual type
    //                 target_sym.set_kind(resolved.kind());
    //                 target_sym.add_depends(resolved, None);
    //                 target_sym.set_is_global(resolved.is_global());
    //                 if let Some(resolved_scope) = resolved.opt_scope()
    //                     && let Some(target_scoped) = target_sym.opt_scope()
    //                 {
    //                     // Build parent scope relationship
    //                     let target_scope = unit.get_scope(target_scoped);
    //                     let resolved_scope = unit.get_scope(resolved_scope);
    //                     target_scope.add_parent(resolved_scope);
    //                     resolved_scope.add_parent(target_scope);
    //                 }
    //             }
    //         }

    //         if let Some(type_node) = node.child_by_field(*unit, LangRust::field_type) {
    //             self.add_type_dependencies_with(unit, &type_node, scopes, target_sym);
    //         }

    //         // Handle trait impl: impl Trait for Type
    //         // Use resolver to properly handle cross-module trait references
    //         // Add dependency FROM the actual struct TO the trait
    //         if let Some(trait_node) = node.child_by_field(*unit, LangRust::field_trait) {
    //             let mut resolver = TyCtxt::new(unit, scopes);
    //             if let Some(trait_sym) = resolver.resolve_type_node(&trait_node)
    //                 && let Some(target_scope) = actual_target.opt_scope()
    //                 && let Some(trait_scope) = trait_sym.opt_scope()
    //             {
    //                 let target_scope = unit.get_scope(target_scope);
    //                 let trait_scope = unit.get_scope(trait_scope);
    //                 target_scope.add_parent(trait_scope);
    //                 actual_target.add_depends_with(trait_sym, DepKind::Implements, None);
    //             }
    //         }
    //     }
    // }

    // fn visit_function_signature_item(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     self.visit_function_item(unit, node, scopes, namespace, parent);
    // }

    // fn visit_macro_definition(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     let sn = node.as_scope().unwrap();
    //     self.visit_scoped_named(unit, node, sn, scopes, namespace, parent);
    // }

    // fn visit_macro_invocation(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     self.visit_children(unit, node, scopes, namespace, parent);

    //     // Get the macro name from the macro_invocation
    //     if let Some(macro_node) = node.child_by_field(*unit, LangRust::field_macro)
    //         && let Some(sym) =
    //             TyCtxt::new(unit, scopes).resolve_expression_symbol(&macro_node, parent)
    //         && let Some(ns) = namespace.opt_symbol()
    //     {
    //         ns.add_depends(sym, Some(&[SymKind::TypeParameter]));
    //     }
    // }

    // fn visit_const_item(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     _parent: Option<&Symbol>,
    // ) {
    //     if let Some(const_ident) = node.child_identifier_by_field(*unit, LangRust::field_name)
    //         && let Some(const_sym) = const_ident.opt_symbol()
    //         && let Some(const_ty) = node.child_by_field(*unit, LangRust::field_type)
    //         && let Some(ty) = {
    //             let mut resolver = TyCtxt::new(unit, scopes);
    //             resolver.resolve_type_node(&const_ty)
    //         }
    //     {
    //         const_sym.set_type_of(ty.id());
    //         const_sym.add_depends(ty, None);
    //         if let Some(ns) = namespace.opt_symbol() {
    //             ns.add_depends(const_sym, Some(&[SymKind::TypeParameter]));
    //         }
    //     }
    // }

    // fn visit_static_item(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     self.visit_const_item(unit, node, scopes, namespace, parent);
    // }

    // fn visit_call_expression(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     self.visit_children(unit, node, scopes, namespace, parent);

    //     let mut resolver = TyCtxt::new(unit, scopes);
    //     let outer_target = resolver.resolve_call_target(node, parent);

    //     if let Some(sym) = outer_target
    //         && let Some(ns) = namespace.opt_symbol()
    //     {
    //         // If call target is an EnumVariant, depend on parent enum instead
    //         if sym.kind() == SymKind::EnumVariant
    //             && let Some(parent_enum_id) = sym.type_of()
    //             && let Some(parent_enum) = unit.opt_get_symbol(parent_enum_id)
    //         {
    //             ns.add_depends_with(
    //                 parent_enum,
    //                 DepKind::Calls,
    //                 Some(&[SymKind::TypeParameter]),
    //             );
    //         } else {
    //             ns.add_depends_with(sym, DepKind::Calls, Some(&[SymKind::TypeParameter]));
    //         }
    //     }

    //     // For scoped calls like Type::method(), also add dependency on the Type
    //     if let Some(func_node) = node.child_by_field(*unit, LangRust::field_function)
    //         && func_node.kind_id() == LangRust::scoped_identifier
    //         && let Some(ns) = namespace.opt_symbol()
    //     {
    //         // Get the path prefix (the type part of Type::method)
    //         if let Some(path_type) = resolver.resolve_scoped_call_receiver(&func_node) {
    //             ns.add_depends(path_type, Some(&[SymKind::TypeParameter]));
    //         }
    //     }

    //     // Add dependencies from outer call target to nested call targets in arguments
    //     if let Some(outer_sym) = outer_target
    //         && let Some(args_node) = node.child_by_field(*unit, LangRust::field_arguments)
    //     {
    //         Self::collect_nested_call_deps(unit, scopes, &args_node, outer_sym, parent);
    //     }
    // }

    // fn visit_block(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     let sn = node.as_scope().unwrap();
    //     scopes.push_scope(sn.scope().id());
    //     self.visit_children(unit, node, scopes, namespace, parent);
    //     scopes.pop_scope();
    // }

    // fn visit_type_item(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     self.visit_children(unit, node, scopes, namespace, parent);

    //     if let Some(type_ident) = node.child_identifier_by_field(*unit, LangRust::field_name)
    //         && let Some(type_sym) = type_ident.opt_symbol()
    //     {
    //         // Add dependency from parent (impl target) to the type alias
    //         if let Some(parent_sym) = parent {
    //             parent_sym.add_depends(type_sym, None);
    //         }

    //         if let Some(type_node) = node.child_by_field(*unit, LangRust::field_type) {
    //             let mut resolver = TyCtxt::new(unit, scopes);
    //             let (ty, args) = resolver.resolve_type_with_args(&type_node);
    //             if let Some(primary) = ty {
    //                 type_sym.set_type_of(primary.id());
    //                 type_sym.add_depends(primary, Some(&[SymKind::TypeParameter]));
    //             }
    //             for arg in &args {
    //                 type_sym.add_depends(arg, Some(&[SymKind::TypeParameter]));
    //             }

    //             for child in node.children(unit) {
    //                 if child.kind_id() == LangRust::where_clause
    //                     || child.kind_id() == LangRust::where_predicate
    //                 {
    //                     self.add_type_dependencies_with(unit, &child, scopes, type_sym);
    //                 }
    //             }

    //             if let Some(ns) = namespace.opt_symbol() {
    //                 Self::add_type_dependencies(ns, ty, &args);
    //             }
    //         }
    //     }
    // }

    // fn visit_type_parameter(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     // Type parameter bounds like `T: Trait` - use TypeBound dependency
    //     if let Some(bounds) = node.child_by_field(*unit, LangRust::field_bounds)
    //         && let Some(owner) = namespace.opt_symbol()
    //     {
    //         self.add_type_bound_with(unit, &bounds, scopes, owner);
    //     }

    //     self.visit_children(unit, node, scopes, namespace, parent);

    //     if let Some(type_ident) = node.child_identifier_by_field(*unit, LangRust::field_name)
    //         && let Some(type_sym) = type_ident.opt_symbol()
    //         && let Some(default_type_node) =
    //             node.child_by_field(*unit, LangRust::field_default_type)
    //     {
    //         let mut resolver = TyCtxt::new(unit, scopes);
    //         let (ty, args) = resolver.resolve_type_with_args(&default_type_node);
    //         if let Some(symbol) = ty {
    //             type_sym.add_depends(symbol, None);

    //             if let Some(ns) = namespace.opt_symbol() {
    //                 ns.add_depends(symbol, Some(&[SymKind::TypeParameter]));
    //             }
    //         }

    //         if let Some(ns) = namespace.opt_symbol() {
    //             for arg in args {
    //                 ns.add_depends(arg, Some(&[SymKind::TypeParameter]));
    //             }
    //         }
    //     }
    // }

    // fn visit_const_parameter(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     self.visit_children(unit, node, scopes, namespace, parent);

    //     let Some(type_node) = node.child_by_field(*unit, LangRust::field_type) else {
    //         return;
    //     };

    //     let mut resolver = TyCtxt::new(unit, scopes);
    //     let (ty, args) = resolver.resolve_type_with_args(&type_node);

    //     if let Some(name_ident) = node.child_identifier_by_field(*unit, LangRust::field_name)
    //         && let Some(symbol) = name_ident.opt_symbol()
    //     {
    //         if let Some(primary) = ty {
    //             symbol.set_type_of(primary.id());
    //             symbol.add_depends(primary, Some(&[SymKind::TypeParameter]));
    //         }
    //         for arg in &args {
    //             symbol.add_depends(arg, Some(&[SymKind::TypeParameter]));
    //         }
    //     }

    //     if let Some(owner) = parent {
    //         Self::add_type_dependencies(owner, ty, &args);
    //     }
    //     if let Some(ns_owner) = namespace.opt_symbol()
    //         && parent.map(|sym| sym.id()) != Some(ns_owner.id())
    //     {
    //         Self::add_type_dependencies(ns_owner, ty, &args);
    //     }
    // }

    // fn visit_associated_type(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     self.visit_children(unit, node, scopes, namespace, parent);

    //     // Add dependency from parent (trait) to the associated type
    //     if let Some(type_ident) = node.child_identifier_by_field(*unit, LangRust::field_name)
    //         && let Some(type_sym) = type_ident.opt_symbol()
    //         && let Some(parent_sym) = parent
    //     {
    //         parent_sym.add_depends(type_sym, None);
    //     }

    //     if let Some(type_ident) = node.child_identifier_by_field(*unit, LangRust::field_name)
    //         && let Some(type_sym) = type_ident.opt_symbol()
    //         && let Some(type_node) = node.child_by_field(*unit, LangRust::field_default_type)
    //     {
    //         let mut resolver = TyCtxt::new(unit, scopes);
    //         let (ty, args) = resolver.resolve_type_with_args(&type_node);
    //         if let Some(primary) = ty {
    //             type_sym.add_depends(primary, Some(&[SymKind::TypeParameter]));
    //         }
    //         for arg in &args {
    //             type_sym.add_depends(arg, Some(&[SymKind::TypeParameter]));
    //         }

    //         if let Some(ns) = namespace.opt_symbol() {
    //             Self::add_type_dependencies(ns, ty, &args);
    //         }
    //     }
    // }

    // fn visit_where_predicate(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     if let Some(bounds) = node.child_by_field(*unit, LangRust::field_bounds)
    //         && let Some(owner) = namespace.opt_symbol()
    //     {
    //         self.add_type_dependencies_with(unit, &bounds, scopes, owner);
    //     }
    //     self.visit_children(unit, node, scopes, namespace, parent);
    // }

    // fn visit_array_type(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     self.visit_children(unit, node, scopes, namespace, parent);

    //     if let Some(element_ty) = node.child_by_field(*unit, LangRust::field_element)
    //         && let Some(owner) = namespace.opt_symbol()
    //     {
    //         self.add_type_dependencies_with(unit, &element_ty, scopes, owner);
    //     }
    // }

    // fn visit_tuple_type(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     self.visit_children(unit, node, scopes, namespace, parent);

    //     let Some(owner) = namespace.opt_symbol() else {
    //         return;
    //     };

    //     for child in node.children(unit) {
    //         if matches!(child.kind(), HirKind::Text | HirKind::Comment) {
    //             continue;
    //         }

    //         self.add_type_dependencies_with(unit, &child, scopes, owner);
    //     }
    // }

    // fn visit_primitive_type(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     self.visit_children(unit, node, scopes, namespace, parent);

    //     let Some(owner) = namespace.opt_symbol() else {
    //         return;
    //     };

    //     self.add_type_dependencies_with(unit, node, scopes, owner);
    // }

    // fn visit_abstract_type(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     self.visit_children(unit, node, scopes, namespace, parent);

    //     if let Some(trait_node) = node.child_by_field(*unit, LangRust::field_trait)
    //         && let Some(owner) = namespace.opt_symbol()
    //     {
    //         self.add_type_dependencies_with(unit, &trait_node, scopes, owner);
    //     }
    // }

    // fn visit_field_declaration(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     self.visit_children(unit, node, scopes, namespace, parent);

    //     let owner_sym = if let Some(sym) = namespace.opt_symbol() {
    //         sym
    //     } else if let Some(parent_sym) = parent
    //         && let Some(resolved) = unit.opt_get_symbol(parent_sym.id())
    //     {
    //         resolved
    //     } else {
    //         return;
    //     };

    //     if let Some(name_node) = node.child_identifier_by_field(*unit, LangRust::field_name)
    //         && let Some(type_node) = node.child_by_field(*unit, LangRust::field_type)
    //     {
    //         let mut resolver = TyCtxt::new(unit, scopes);
    //         if let Some((symbol, _)) = resolver.resolve_field_type(owner_sym, &name_node.name) {
    //             let (ty, args) = resolver.resolve_type_with_args(&type_node);
    //             if let Some(primary) = ty {
    //                 symbol.set_type_of(primary.id());
    //                 symbol.add_depends_with(
    //                     primary,
    //                     DepKind::FieldType,
    //                     Some(&[SymKind::TypeParameter]),
    //                 );
    //             }
    //             for arg in &args {
    //                 symbol.add_depends_with(
    //                     arg,
    //                     DepKind::FieldType,
    //                     Some(&[SymKind::TypeParameter]),
    //                 );
    //             }

    //             Self::add_type_dependencies_with_kind(owner_sym, ty, &args, DepKind::FieldType);
    //         }
    //     }
    // }

    // fn visit_enum_variant(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     self.visit_children(unit, node, scopes, namespace, parent);

    //     if let Some(name_node) = node.child_identifier_by_field(*unit, LangRust::field_name)
    //         && let Some(symbol) = name_node.opt_symbol()
    //     {
    //         if let Some(ns) = namespace.opt_symbol() {
    //             ns.add_depends(symbol, Some(&[SymKind::TypeParameter]));
    //         }

    //         if let Some(value_node) = node.child_by_field(*unit, LangRust::field_value) {
    //             let mut resolver = TyCtxt::new(unit, scopes);
    //             let (ty, args) = resolver.resolve_type_with_args(&value_node);
    //             if let Some(primary) = ty {
    //                 symbol.add_depends(primary, Some(&[SymKind::TypeParameter]));
    //             }

    //             for arg in &args {
    //                 symbol.add_depends(arg, Some(&[SymKind::TypeParameter]));
    //             }

    //             // Use FieldType for enum variant inner types so they appear in arch-graph
    //             if let Some(ns) = namespace.opt_symbol()
    //                 && let Some(primary) = ty
    //             {
    //                 ns.add_depends_with(
    //                     primary,
    //                     DepKind::FieldType,
    //                     Some(&[SymKind::TypeParameter]),
    //                 );
    //             }
    //         }

    //         // Handle tuple-like enum variants: Root(&'hir HirRoot)
    //         // The type is in the body field as ordered_field_declaration_list
    //         if let Some(body_node) = node.child_by_field(*unit, LangRust::field_body)
    //             && let Some(ns) = namespace.opt_symbol()
    //         {
    //             let mut resolver = TyCtxt::new(unit, scopes);
    //             let (ty, args) = resolver.resolve_type_with_args(&body_node);
    //             Self::add_type_dependencies_with_kind(ns, ty, &args, DepKind::FieldType);
    //         }
    //     }
    // }

    // fn visit_parameter(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     self.visit_children(unit, node, scopes, namespace, parent);

    //     if let Some(type_node) = node.child_by_field(*unit, LangRust::field_type) {
    //         let mut resolver = TyCtxt::new(unit, scopes);
    //         let (ty, args) = resolver.resolve_type_with_args(&type_node);

    //         if let Some(pattern) = node.child_by_field(*unit, LangRust::field_pattern)
    //             && let Some(ident) = pattern.find_identifier(*unit)
    //             && let Some(symbol) = ident.opt_symbol()
    //         {
    //             if let Some(primary) = ty {
    //                 symbol.set_type_of(primary.id());
    //                 symbol.add_depends_with(
    //                     primary,
    //                     DepKind::ParamType,
    //                     Some(&[SymKind::TypeParameter]),
    //                 );
    //             }
    //             for arg in &args {
    //                 symbol.add_depends_with(
    //                     arg,
    //                     DepKind::ParamType,
    //                     Some(&[SymKind::TypeParameter]),
    //                 );
    //             }
    //         }

    //         if let Some(owner) = parent {
    //             Self::add_type_dependencies_with_kind(owner, ty, &args, DepKind::ParamType);
    //         }

    //         if let Some(ns) = namespace.opt_symbol() {
    //             Self::add_type_dependencies_with_kind(ns, ty, &args, DepKind::ParamType);
    //         }
    //     }
    // }

    // fn visit_self_parameter(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     self.visit_children(unit, node, scopes, namespace, parent);

    //     let Some(type_node) = node.child_by_field(*unit, LangRust::field_type) else {
    //         return;
    //     };

    //     let mut resolver = TyCtxt::new(unit, scopes);
    //     let (ty, args) = resolver.resolve_type_with_args(&type_node);

    //     if let Some(ns_owner) = namespace.opt_symbol() {
    //         Self::add_type_dependencies(ns_owner, ty, &args);
    //     }

    //     if let Some(parent_owner) = parent
    //         && namespace.opt_symbol().map(|sym| sym.id()) != Some(parent_owner.id())
    //     {
    //         Self::add_type_dependencies(parent_owner, ty, &args);
    //     }
    // }

    // fn visit_identifier(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     _parent: Option<&Symbol>,
    // ) {
    //     Self::add_const_dependency(unit, scopes, node, namespace);
    // }

    // fn visit_scoped_identifier(
    //     &mut self,
    //     unit: &CompileUnit<'tcx>,
    //     node: &HirNode<'tcx>,
    //     scopes: &mut BinderScopes<'tcx>,
    //     namespace: &'tcx Scope<'tcx>,
    //     parent: Option<&Symbol>,
    // ) {
    //     // First try the standard const dependency check (for simple name lookups)
    //     Self::add_const_dependency(unit, scopes, node, namespace);

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
        "#;

        with_compiled_unit(&[source], |cc| {
            assert_depends(
                cc,
                "get_value",
                SymKind::Function,
                "Option",
                SymKind::Struct,
                Some(DepKind::ReturnType),
            );
            assert_depends(
                cc,
                "get_value",
                SymKind::Function,
                "i32",
                SymKind::Primitive,
                Some(DepKind::ReturnType),
            );
        });
    }
}
