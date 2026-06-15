use llmcc_core::ResolveOptions;
use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirNode, HirScope};
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{
    SYM_KIND_ALL, SYM_KIND_CALLABLE, SYM_KIND_IMPL_TARGETS, SYM_KIND_TYPES, SymKind, SymKindSet,
    Symbol,
};
use llmcc_resolver::BindCtxt;

use crate::infer::infer_type;
use crate::pattern::bind_pattern_types;
use crate::token::AstVisitorRust;
use crate::token::LangRust;

type ScopeHook<'tcx> =
    Box<dyn FnOnce(&CompileUnit<'tcx>, &'tcx HirScope<'tcx>, &mut BindCtxt<'tcx>) + 'tcx>;

/// Visitor for resolving symbol bindings and establishing relationships.
#[derive(Debug)]
struct BinderVisitor;

impl BinderVisitor {
    fn new() -> Self {
        Self
    }

    /// Enter a named semantic scope and restore the previous binding depth.
    fn visit_named_scope<'tcx>(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        sn: &'tcx HirScope<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        parent: Option<&Symbol>,
        on_scope_enter: Option<ScopeHook<'tcx>>,
    ) {
        let depth = scopes.depth();

        let child_parent = sn.try_symbol().or(parent);

        scopes.push_node_scope(sn);

        if let Some(scope_enter) = on_scope_enter {
            scope_enter(unit, sn, scopes);
        }
        self.visit_children(unit, node, scopes, scopes.current(), child_parent);
        scopes.pop_to(depth);
    }
}

impl<'tcx> AstVisitorRust<'tcx, BindCtxt<'tcx>> for BinderVisitor {
    fn visit_source_file(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let depth = scopes.depth();
        let meta = unit.unit_meta();

        if let Some(ref crate_name) = meta.package_name
            && let Some(symbol) =
                scopes.lookup_symbol(crate_name, SymKindSet::from_kind(SymKind::Package))
            && let Some(scope_id) = symbol.try_owned_scope()
        {
            scopes.push_scope(scope_id);
        }

        if let Some(ref module_name) = meta.module_name
            && let Some(symbol) =
                scopes.lookup_symbol(module_name, SymKindSet::from_kind(SymKind::Module))
            && let Some(scope_id) = symbol.try_owned_scope()
        {
            scopes.push_scope(scope_id);
        }

        if let Some(ref file_name) = meta.file_name
            && let Some(file_sym) =
                scopes.lookup_symbol(file_name, SymKindSet::from_kind(SymKind::File))
            && let Some(scope_id) = file_sym.try_owned_scope()
        {
            scopes.push_scope(scope_id);

            let file_scope = unit.scope(scope_id);
            self.visit_children(unit, node, scopes, file_scope, Some(file_sym));
            scopes.pop_to(depth);
        }
    }

    fn visit_identifier(
        &mut self,
        _unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let Some(ident) = node.as_ident() else {
            return;
        };
        if let Some(existing) = ident.try_symbol()
            && existing.kind().is_resolved()
        {
            return;
        }

        if let Some(symbol) = scopes.lookup_symbol(ident.name, SYM_KIND_ALL) {
            ident.set_symbol(symbol);
        }
    }

    fn visit_type_identifier(
        &mut self,
        _unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let Some(ident) = node.as_ident() else {
            return;
        };
        if let Some(existing) = ident.try_symbol()
            && existing.kind().is_resolved()
        {
            return;
        }

        if let Some(symbol) = scopes.lookup_symbol(ident.name, SYM_KIND_TYPES) {
            ident.set_symbol(symbol);
        }
    }

    fn visit_primitive_type(
        &mut self,
        _unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let Some(ident) = node.as_ident() else {
            return;
        };
        if let Some(symbol) =
            scopes.lookup_global(ident.name, SymKindSet::from_kind(SymKind::Primitive))
        {
            ident.set_symbol(symbol);
        }
    }

    /// Type parameters remember their first bound or default type when present.
    fn visit_type_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(type_param_sym) = node.query(unit).try_resolved_by_field(LangRust::field_name) {
            if let Some(bounds_node) = node.child_by_field(unit, LangRust::field_bounds) {
                if let Some(first_bound) = infer_type(unit, scopes, &bounds_node) {
                    type_param_sym.set_type_of(first_bound.id());
                    return;
                }
            }
            if let Some(default_node) = node.child_by_field(unit, LangRust::field_default_type) {
                if let Some(default_type) = infer_type(unit, scopes, &default_node) {
                    type_param_sym.set_type_of(default_type.id());
                }
            }
        }
    }

    /// Anonymous blocks may have scopes, but fall back to lexical traversal if not collected.
    fn visit_block(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let Some(sn) = node.as_scope() else {
            self.visit_children(unit, node, scopes, namespace, parent);
            return;
        };

        let depth = scopes.depth();
        if scopes.push_node_scope(sn) {
            self.visit_children(unit, node, scopes, scopes.current(), parent);
            scopes.pop_to(depth);
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    fn visit_mod_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if node.child_by_field(unit, LangRust::field_body).is_none() {
            return;
        }

        let Some(sn) = node.as_scope() else {
            self.visit_children(unit, node, scopes, namespace, parent);
            return;
        };
        self.visit_named_scope(unit, node, sn, scopes, parent, None);
    }

    fn visit_function_signature_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_function_item(unit, node, scopes, namespace, parent);
    }

    /// Bind function bodies and attach resolved return/nested type metadata.
    fn visit_function_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let Some(sn) = node.as_scope() else {
            self.visit_children(unit, node, scopes, namespace, parent);
            return;
        };
        self.visit_named_scope(unit, node, sn, scopes, parent, None);

        let Some(fn_sym) = sn.try_symbol() else {
            return;
        };

        if let Some(return_type) = node
            .query(unit)
            .try_resolved_by_field(LangRust::field_return_type)
        {
            fn_sym.set_type_of(return_type.id());
        }

        // Generic return arguments become graph dependency candidates.
        if let Some(return_type_node) = node.child_by_field(unit, LangRust::field_return_type) {
            extract_nested_types(unit, scopes, &return_type_node, fn_sym);
        }
    }

    fn visit_field_identifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_identifier(unit, node, scopes, namespace, parent);
    }

    /// Struct bodies bind `Self`/`self` aliases to the struct symbol.
    fn visit_struct_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let Some(sn) = node.as_scope() else {
            self.visit_children(unit, node, scopes, namespace, parent);
            return;
        };
        self.visit_named_scope(
            unit,
            node,
            sn,
            scopes,
            parent,
            Some(Box::new(|_unit, sn, scopes| {
                for key in ["Self", "self"] {
                    if let Some(self_sym) =
                        scopes.lookup_symbol(key, SymKindSet::from_kind(SymKind::TypeAlias))
                        && let Some(struct_sym) = sn.try_symbol()
                    {
                        self_sym.set_type_of(struct_sym.id());
                        if let Some(struct_scope) = struct_sym.try_owned_scope() {
                            self_sym.set_owned_scope(struct_scope);
                        }
                    }
                }
            })),
        );
    }

    /// Trait bodies bind `Self`/`self` aliases to the trait symbol.
    fn visit_trait_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let Some(sn) = node.as_scope() else {
            self.visit_children(unit, node, scopes, namespace, parent);
            return;
        };
        self.visit_named_scope(
            unit,
            node,
            sn,
            scopes,
            parent,
            Some(Box::new(|_unit, sn, scopes| {
                for key in ["Self", "self"] {
                    if let Some(self_sym) =
                        scopes.lookup_symbol(key, SymKindSet::from_kind(SymKind::TypeAlias))
                        && let Some(trait_sym) = sn.try_symbol()
                    {
                        self_sym.set_type_of(trait_sym.id());
                        if let Some(trait_scope) = trait_sym.try_owned_scope() {
                            self_sym.set_owned_scope(trait_scope);
                        }
                    }
                }
            })),
        );
    }

    /// Field declarations record owner type and declared field type.
    fn visit_field_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(field_sym) = node.query(unit).try_resolved_by_field(LangRust::field_name) {
            if let Some(struct_sym) = namespace.try_symbol() {
                field_sym.set_field_of(struct_sym.id());
            }

            if let Some(type_node) = node.child_by_field(unit, LangRust::field_type)
                && let Some(field_type) = infer_type(unit, scopes, &type_node)
            {
                field_sym.set_type_of(field_type.id());
                if let Some(struct_sym) = namespace.try_symbol() {
                    struct_sym.add_nested_type(field_type.id());
                }
                // Keep generic arguments on the field, not the enclosing struct,
                // so graph edges describe the field type relation precisely.
                extract_nested_types(unit, scopes, &type_node, field_sym);
            }
        }
    }

    /// Impl blocks connect target types, optional trait scopes, and generic arguments.
    fn visit_impl_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let target_ident = node.query(unit).try_ident_with_field(LangRust::field_type);
        if let Some(target_ident) = target_ident
            && let Some(target_sym) = target_ident.try_symbol()
        {
            let target_resolved = scopes.lookup_symbol(target_ident.name, SYM_KIND_IMPL_TARGETS);

            if target_sym.kind() == SymKind::UnresolvedType
                && let Some(resolved) = target_resolved
                && resolved.id() != target_sym.id()
                && !matches!(resolved.kind(), SymKind::UnresolvedType)
            {
                target_sym.set_type_of(resolved.id());
                target_sym.set_kind(resolved.kind());
                target_sym.set_is_global(resolved.is_global());

                if let Some(resolved_scope) = resolved.try_owned_scope()
                    && let Some(target_scoped) = target_sym.try_owned_scope()
                {
                    let target_scope = unit.scope(target_scoped);
                    let resolved_scope = unit.scope(resolved_scope);
                    target_scope.add_parent(resolved_scope);
                    resolved_scope.add_parent(target_scope);
                }
            }

            if let Some(trait_node) = node.child_by_field(unit, LangRust::field_trait)
                && let Some(trait_ident) = trait_node.query(unit).try_first_ident()
            {
                // Avoid SYM_KIND_TYPES here: a type parameter with the same name
                // is not an implementation contract.
                let trait_sym =
                    scopes.lookup_symbol(trait_ident.name, SymKindSet::from_kind(SymKind::Trait));

                if let Some(trait_sym) = trait_sym {
                    trait_ident.set_symbol(trait_sym);

                    if let Some(target_resolved) = target_resolved
                        && let Some(target_scope) = target_resolved.try_owned_scope()
                        && let Some(trait_scope) = trait_sym.try_owned_scope()
                    {
                        let target_scope = unit.scope(target_scope);
                        let trait_scope = unit.scope(trait_scope);
                        target_scope.add_parent(trait_scope);
                    }
                }

                // Trait generic arguments become dependency candidates on the impl target.
                if let Some(type_args) =
                    trait_node.child_by_field(unit, LangRust::field_type_arguments)
                {
                    for &child_id in type_args.child_ids() {
                        let child = unit.hir_node(child_id);
                        if child.is_trivia() || child.kind_id() == LangRust::lifetime {
                            continue;
                        }
                        if let Some(type_arg_sym) = infer_type(unit, scopes, &child) {
                            if type_arg_sym.kind().is_defined_type() {
                                target_sym.add_nested_type(type_arg_sym.id());
                            }
                        }
                    }
                }
            }

            let Some(sn) = node.as_scope() else {
                self.visit_children(unit, node, scopes, scopes.current(), Some(target_sym));
                return;
            };
            if target_sym
                .try_owned_scope()
                .and_then(|scope_id| unit.try_scope(scope_id))
                .is_none()
            {
                self.visit_children(unit, node, scopes, scopes.current(), Some(target_sym));
                return;
            }
            self.visit_named_scope(unit, node, sn, scopes, Some(target_sym), None);
        }
    }

    /// Calls resolve their callee through the child expression visitor first.
    fn visit_call_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // Simple callee identifiers sometimes need a callable-only fallback.
        if let Some(func_node) = node.child_by_field(unit, LangRust::field_function)
            && func_node.kind_id() == LangRust::identifier
            && let Some(ident) = func_node.as_ident()
            && ident.try_symbol().is_none()
            && let Some(symbol) = scopes.lookup_symbol(ident.name, SYM_KIND_CALLABLE)
        {
            ident.set_symbol(symbol);
        }
    }

    fn visit_enum_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let Some(sn) = node.as_scope() else {
            self.visit_children(unit, node, scopes, namespace, parent);
            return;
        };
        self.visit_named_scope(unit, node, sn, scopes, parent, None);
    }

    fn visit_macro_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let Some(sn) = node.as_scope() else {
            self.visit_children(unit, node, scopes, namespace, parent);
            return;
        };
        self.visit_named_scope(unit, node, sn, scopes, parent, None);
    }

    fn visit_macro_invocation(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_generic_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    /// Resolve `use path as alias` aliases after the imported path is bound.
    fn visit_use_as_clause(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        let Some(alias) = node.query(unit).try_ident_with_field(LangRust::field_alias) else {
            return;
        };
        let Some(alias_symbol) = alias.try_symbol().or_else(|| {
            scopes.lookup_symbol(alias.name, SymKindSet::from_kind(SymKind::TypeAlias))
        }) else {
            return;
        };
        let Some(path) = node.child_by_field(unit, LangRust::field_path) else {
            return;
        };
        let Some(target) =
            infer_type(unit, scopes, &path).or_else(|| path.query(unit).try_resolved())
        else {
            return;
        };

        alias_symbol.set_type_of(target.id());
        if let Some(scope_id) = target.try_owned_scope() {
            alias_symbol.set_owned_scope(scope_id);
        }
    }

    /// Const/static declarations attach their declared type when available.
    fn visit_const_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(const_sym) = node.query(unit).try_resolved_by_field(LangRust::field_name)
            && let Some(const_ty) = node.child_by_field(unit, LangRust::field_type)
            && let Some(ty) = infer_type(unit, scopes, &const_ty)
        {
            const_sym.set_type_of(ty.id());
        }
    }

    fn visit_static_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_const_item(unit, node, scopes, namespace, parent);
    }

    /// Type aliases point at their resolved target type.
    fn visit_type_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(name_sym) = node.query(unit).try_resolved()
            && let Some(type_sym) = node.query(unit).try_resolved_by_field(LangRust::field_type)
        {
            name_sym.set_type_of(type_sym.id());
        }
    }

    /// Composite array symbols collect their element type.
    fn visit_array_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(sn) = node.as_scope()
            && let Some(ident) = sn.try_ident()
            && let Some(symbol) =
                scopes.lookup_symbol(ident.name, SymKindSet::from_kind(SymKind::CompositeType))
            && symbol.nested_types().is_none()
        {
            if let Some(array_type_sym) = node
                .query(unit)
                .try_resolved_by_field(LangRust::field_element)
            {
                symbol.add_nested_type(array_type_sym.id());
            }
        }
    }

    /// Composite tuple symbols collect ordered element types.
    fn visit_tuple_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(sn) = node.as_scope()
            && let Some(tuple_ident) = sn.try_ident()
            && let Some(tuple_symbol) = scopes.lookup_symbol(
                tuple_ident.name,
                SymKindSet::from_kind(SymKind::CompositeType),
            )
            && tuple_symbol.nested_types().is_none()
        {
            for type_ident in node.query(unit).identifiers() {
                if let Some(type_sym) = type_ident.try_symbol() {
                    tuple_symbol.add_nested_type(type_sym.id());
                }
            }
        }
    }

    fn visit_abstract_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_enum_variant(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_struct_item(unit, node, scopes, namespace, parent);
    }

    /// Field expressions record owner/type links for named and tuple fields.
    fn visit_field_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(value_sym) = node
            .query(unit)
            .try_resolved_by_field(LangRust::field_value)
            && let Some(field_node) = node.child_by_field(unit, LangRust::field_field)
        {
            if field_node.kind_id() == LangRust::integer_literal {
                if let Some(field_sym) = field_node.query(unit).try_resolved() {
                    field_sym.set_field_of(value_sym.id());

                    if let Some(text) = field_node.as_text().map(|s| s.text()) {
                        if let Ok(index) = text.parse::<usize>() {
                            if let Some(value_type_id) = value_sym.type_of()
                                && let Some(value_type) = unit.try_symbol(value_type_id)
                                && let Some(nested) = value_type.nested_types()
                                && index < nested.len()
                            {
                                field_sym.set_type_of(nested[index]);
                            }
                        }
                    }
                }
            } else if let Some(field_ident) = field_node.query(unit).try_first_ident() {
                if let Some(field_sym) = field_ident.try_symbol() {
                    field_sym.set_field_of(value_sym.id());
                }
            }
        }
    }

    /// Resolve `path::name` by first resolving the path owner, then member lookup.
    fn visit_scoped_identifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        let name_ident = node.query(unit).try_ident_with_field(LangRust::field_name);
        let path_node = node.child_by_field(unit, LangRust::field_path);

        if let Some(name_ident) = name_ident
            && let Some(path_node) = path_node
        {
            let path_sym = if path_node.kind_id() == LangRust::scoped_identifier {
                path_node
                    .query(unit)
                    .try_ident_with_field(LangRust::field_name)
                    .and_then(|i| i.try_symbol())
            } else if path_node.kind_id() == LangRust::identifier {
                let path_ident = path_node.as_ident();
                if let Some(ident) = path_ident {
                    if let Some(sym) = ident.try_symbol() {
                        Some(sym)
                    } else {
                        let sym = scopes.lookup_symbol(ident.name, SymKindSet::empty());
                        if let Some(s) = sym {
                            ident.set_symbol(s);
                        }
                        sym
                    }
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(path_sym) = path_sym {
                if let Some(name_sym) =
                    scopes.lookup_member(path_sym, name_ident.name, SymKindSet::empty())
                {
                    name_ident.set_symbol(name_sym);
                }
            }
        }
    }

    /// Parameter annotations flow into their binding pattern.
    fn visit_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(type_node) = node.child_by_field(unit, LangRust::field_type)
            && let Some(pattern) = node
                .query(unit)
                .try_descendant_with_field(LangRust::field_pattern)
        {
            if let Some(type_sym) = infer_type(unit, scopes, &type_node) {
                bind_pattern_types(unit, scopes, &pattern, type_sym);
            }
        }
    }

    /// `self` parameters use the impl-scope `self` type alias collected earlier.
    fn visit_self_param(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(self_sym) =
            scopes.lookup_symbol("self", SymKindSet::from_kind(SymKind::TypeAlias))
        {
            for &child_id in node.child_ids() {
                let child = unit.hir_node(child_id);
                if let Some(ident) = child.as_ident() {
                    if ident.name == "self" {
                        ident.set_symbol(self_sym);
                    }
                }
            }
        }
    }

    fn visit_scoped_type_identifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_scoped_identifier(unit, node, scopes, namespace, parent);
    }

    /// Let declarations propagate explicit or inferred value types into patterns.
    fn visit_let_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(type_node) = node.child_by_field(unit, LangRust::field_type)
            && let Some(pattern) = node
                .query(unit)
                .try_descendant_with_field(LangRust::field_pattern)
        {
            if let Some(type_sym) = infer_type(unit, scopes, &type_node) {
                bind_pattern_types(unit, scopes, &pattern, type_sym);
                return;
            }
            if let Some(type_sym) = type_node.query(unit).try_resolved() {
                bind_pattern_types(unit, scopes, &pattern, type_sym);
                return;
            }
        }

        if let Some(value_node) = node.child_by_field(unit, LangRust::field_value)
            && let Some(pattern) = node
                .query(unit)
                .try_descendant_with_field(LangRust::field_pattern)
            && let Some(type_sym) = infer_type(unit, scopes, &value_node)
        {
            bind_pattern_types(unit, scopes, &pattern, type_sym);
        }
    }

    fn visit_tuple_struct_pattern(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_struct_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_match_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let scrutinee_type = node
            .child_by_field(unit, LangRust::field_value)
            .and_then(|value| infer_type(unit, scopes, &value));

        self.visit_children(unit, node, scopes, namespace, parent);

        let Some(scrutinee_type) = scrutinee_type else {
            return;
        };
        let Some(body) = node.child_by_field(unit, LangRust::field_body) else {
            return;
        };

        for arm in body.children(unit) {
            if let Some(pattern) = arm.child_by_field(unit, LangRust::field_pattern) {
                bind_pattern_types(unit, scopes, &pattern, scrutinee_type);
            }
        }
    }

    fn visit_match_block(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_block(unit, node, scopes, namespace, parent);
    }
}

/// Add generic/scoped type arguments as graph dependency candidates.
fn extract_nested_types<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BindCtxt<'tcx>,
    type_node: &HirNode<'tcx>,
    target_sym: &Symbol,
) {
    if matches!(
        type_node.kind_id(),
        LangRust::generic_type | LangRust::scoped_type_identifier
    ) {
        if let Some(type_args) = type_node.child_by_field(unit, LangRust::field_type_arguments) {
            for &child_id in type_args.child_ids() {
                let child = unit.hir_node(child_id);
                if child.is_trivia() || child.kind_id() == LangRust::lifetime {
                    continue;
                }
                if let Some(type_arg_sym) = infer_type(unit, scopes, &child) {
                    target_sym.add_nested_type(type_arg_sym.id());
                }
                extract_nested_types(unit, scopes, &child, target_sym);
            }
        }
    }
}

pub(crate) fn bind_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    namespace: &'tcx Scope<'tcx>,
    _config: &ResolveOptions,
) {
    let mut scopes = BindCtxt::new(unit, namespace);
    let mut visit = BinderVisitor::new();
    visit.visit_node(&unit, node, &mut scopes, namespace, None);
}
