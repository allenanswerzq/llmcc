use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirKind, HirNode, HirScope};
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{SymKind, Symbol};
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
    config: ResolverOption,
    phantom: std::marker::PhantomData<&'tcx ()>,
}

impl<'tcx> BinderVisitor<'tcx> {
    fn new(config: ResolverOption) -> Self {
        Self {
            config,
            phantom: std::marker::PhantomData,
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

        if let Some(fn_sym) = sn.opt_symbol() {
            let fn_name = unit.interner().resolve_owned(fn_sym.name);
            tracing::trace!("func: {}", fn_name.as_deref().unwrap_or("?"));

            // Mark main function as global (entry point)
            if fn_name.as_deref() == Some("main") {
                tracing::trace!("marking main function as global");
                fn_sym.set_is_global(true);
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
            }

            let sn = node.as_scope().unwrap();
            let target_scope = unit.get_scope(target_sym.opt_scope().unwrap());
            self.visit_scoped_named(unit, node, sn, scopes, target_scope, Some(target_sym), None);
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
    }

    fn visit_const_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        if let Some(const_ident) = node.ident_by_field(*unit, LangRust::field_name)
            && let Some(const_ty) = node.child_by_field(*unit, LangRust::field_type)
            && let Some(const_sym) = const_ident.opt_symbol()
            && let Some(ty) = TyCtxt::new(unit, scopes).resolve_type(&const_ty)
        {
            const_sym.set_type_of(ty.id());
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
    }
}

pub fn bind_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    namespace: &'tcx Scope<'tcx>,
    config: &ResolverOption,
) {
    let mut scopes = BinderScopes::new(unit, namespace);
    let mut visit = BinderVisitor::new(config.clone());
    visit.visit_node(&unit, node, &mut scopes, namespace, None);
}
