#![allow(clippy::collapsible_if, clippy::needless_return)]

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirNode, HirScope};
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{
    SYM_KIND_ALL, SYM_KIND_CALLABLE, SYM_KIND_IMPL_TARGETS, SYM_KIND_TYPES, SymKind, SymKindSet,
    Symbol,
};
use llmcc_resolver::{BinderScopes, ResolverOption};

use crate::infer::infer_type;
use crate::pattern::bind_pattern_types;
use crate::token::AstVisitorRust;
use crate::token::LangRust;

type ScopeEnterFn<'tcx> =
    Box<dyn FnOnce(&CompileUnit<'tcx>, &'tcx HirScope<'tcx>, &mut BinderScopes<'tcx>) + 'tcx>;

/// Visitor for resolving symbol bindings and establishing relationships.
#[derive(Debug)]
pub struct BinderVisitor<'tcx> {
    #[allow(dead_code)]
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

    #[allow(clippy::too_many_arguments)]
    // AST: Helper for named scope nodes (struct, enum, mod, impl, etc.)
    fn visit_scoped_named(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        sn: &'tcx HirScope<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
        on_scope_enter: Option<ScopeEnterFn<'tcx>>,
    ) {
        let depth = scopes.scope_depth();

        // Any visibility modifier (pub, pub(crate), pub(super), etc.) makes the symbol global
        if let Some(_vis_modifier) = node.child_by_kind(unit, LangRust::visibility_modifier)
            && let Some(sym) = sn.opt_symbol()
        {
            sym.set_is_global(true);
            scopes.globals().insert(sym);
        }

        let child_parent = sn.opt_symbol().or(parent);

        // Push scope (always succeeds for Rust since collector sets all scopes)
        scopes.push_scope_node(sn);

        if let Some(scope_enter) = on_scope_enter {
            scope_enter(unit, sn, scopes);
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
        let _file_path = unit.file_path().unwrap();
        let depth = scopes.scope_depth();
        let meta = unit.unit_meta();

        if let Some(ref crate_name) = meta.package_name
            && let Some(symbol) =
                scopes.lookup_symbol(crate_name, SymKindSet::from_kind(SymKind::Crate))
            && let Some(scope_id) = symbol.opt_scope()
        {
            scopes.push_scope(scope_id);
        }

        if let Some(ref module_name) = meta.module_name
            && let Some(symbol) =
                scopes.lookup_symbol(module_name, SymKindSet::from_kind(SymKind::Module))
            && let Some(scope_id) = symbol.opt_scope()
        {
            scopes.push_scope(scope_id);
        }

        if let Some(ref file_name) = meta.file_name
            && let Some(file_sym) =
                scopes.lookup_symbol(file_name, SymKindSet::from_kind(SymKind::File))
            && let Some(scope_id) = file_sym.opt_scope()
        {
            scopes.push_scope(scope_id);

            let file_scope = unit.get_scope(scope_id);
            self.visit_children(unit, node, scopes, file_scope, Some(file_sym));
            scopes.pop_until(depth);
        }
    }

    // AST: identifier (variable, function name, type name, etc.)
    fn visit_identifier(
        &mut self,
        _unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let ident = node.as_ident().unwrap();
        if let Some(existing) = ident.opt_symbol()
            && existing.kind().is_resolved()
        {
            return;
        }

        if let Some(symbol) = scopes.lookup_symbol(ident.name, SYM_KIND_ALL) {
            ident.set_symbol(symbol);
        }
    }

    // AST: type_identifier (refers to struct, enum, trait, etc.)
    fn visit_type_identifier(
        &mut self,
        _unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let ident = node.as_ident().unwrap();
        if let Some(existing) = ident.opt_symbol()
            && existing.kind().is_resolved()
        {
            return;
        }

        if let Some(symbol) = scopes.lookup_symbol(ident.name, SYM_KIND_TYPES) {
            ident.set_symbol(symbol);
        }
    }

    // AST: primitive_type (i32, u64, bool, f32, str, etc.)
    fn visit_primitive_type(
        &mut self,
        _unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let ident = node.as_ident().unwrap();
        if let Some(symbol) =
            scopes.lookup_global(ident.name, SymKindSet::from_kind(SymKind::Primitive))
        {
            ident.set_symbol(symbol);
        }
    }

    // AST: type parameter T or T: Trait or T=Default in generics
    // Sets type_of on the type parameter to point to its first trait bound or default type
    fn visit_type_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // Get the type parameter symbol
        if let Some(type_param_sym) = node.ident_symbol_by_field(unit, LangRust::field_name) {
            // Priority 1: Look for trait bounds (T: Trait)
            if let Some(bounds_node) = node.child_by_field(unit, LangRust::field_bounds) {
                if let Some(first_bound) = infer_type(unit, scopes, &bounds_node) {
                    type_param_sym.set_type_of(first_bound.id());
                    return;
                }
            }
            // Priority 2: Look for default type (RHS=Self)
            if let Some(default_node) = node.child_by_field(unit, LangRust::field_default_type) {
                if let Some(default_type) = infer_type(unit, scopes, &default_node) {
                    type_param_sym.set_type_of(default_type.id());
                }
            }
        }
    }

    // AST: block { ... statements ... }
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

    // AST: mod name { ... items ... }
    fn visit_mod_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if node.child_by_field(unit, LangRust::field_body).is_none() {
            return;
        }

        let sn = node.as_scope().unwrap();
        self.visit_scoped_named(unit, node, sn, scopes, namespace, parent, None);
    }

    // AST: fn foo(args) -> ret_type;
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

    // AST: fn name(args) -> ret_type { body }
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

        let Some(fn_sym) = sn.opt_symbol() else {
            return;
        };

        if let Some(return_type) = node.ident_symbol_by_field(unit, LangRust::field_return_type) {
            fn_sym.set_type_of(return_type.id());
        }

        // Extract nested types from generic return types (e.g., Result<User, Error>)
        if let Some(return_type_node) = node.child_by_field(unit, LangRust::field_return_type) {
            extract_nested_types(unit, scopes, &return_type_node, fn_sym);
        }

        if unit.resolve_name(fn_sym.name) == "main" {
            fn_sym.set_is_global(true);
            scopes.globals().insert(fn_sym);
        }
    }

    // AST: field_identifier (struct field name)
    fn visit_field_identifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_identifier(unit, node, scopes, namespace, parent);
    }

    // AST: struct Name { field1: Type1, field2: Type2 }
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
            Some(Box::new(|_unit, sn, scopes| {
                for key in ["Self", "self"] {
                    if let Some(self_sym) =
                        scopes.lookup_symbol(key, SymKindSet::from_kind(SymKind::TypeAlias))
                        && let Some(struct_sym) = sn.opt_symbol()
                    {
                        self_sym.set_type_of(struct_sym.id());
                        if let Some(struct_scope) = struct_sym.opt_scope() {
                            self_sym.set_scope(struct_scope);
                        }
                    }
                }
            })),
        );
    }

    // AST: trait Name { methods... }
    // Purpose: Bind Self/self to the trait for methods inside the trait
    fn visit_trait_item(
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
            Some(Box::new(|_unit, sn, scopes| {
                for key in ["Self", "self"] {
                    if let Some(self_sym) =
                        scopes.lookup_symbol(key, SymKindSet::from_kind(SymKind::TypeAlias))
                        && let Some(trait_sym) = sn.opt_symbol()
                    {
                        self_sym.set_type_of(trait_sym.id());
                        if let Some(trait_scope) = trait_sym.opt_scope() {
                            self_sym.set_scope(trait_scope);
                        }
                    }
                }
            })),
        );
    }

    // AST: field: Type
    fn visit_field_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(field_sym) = node.ident_symbol_by_field(unit, LangRust::field_name) {
            if let Some(struct_sym) = namespace.opt_symbol() {
                field_sym.set_field_of(struct_sym.id());
            }

            if let Some(type_node) = node.child_by_field(unit, LangRust::field_type)
                && let Some(field_type) = infer_type(unit, scopes, &type_node)
            {
                field_sym.set_type_of(field_type.id());
                if let Some(struct_sym) = namespace.opt_symbol() {
                    struct_sym.add_nested_type(field_type.id());
                }
                // Extract nested types from generic field types to the FIELD symbol
                // e.g., for `data: Triple<User, Error>`, add User/Error to field's nested_types
                // This allows edges: User -> Triple, Error -> Triple (not User -> AllThree)
                extract_nested_types(unit, scopes, &type_node, field_sym);
            }
        }
    }

    // AST: impl [<Trait> for] Type { methods }
    fn visit_impl_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let target_ident = node.ident_by_field(unit, LangRust::field_type);
        if let Some(target_ident) = target_ident
            && let Some(target_sym) = target_ident.opt_symbol()
        {
            // Look up the impl target type (struct or enum that the trait is implemented for)
            let target_resolved = scopes.lookup_symbol(target_ident.name, SYM_KIND_IMPL_TARGETS);

            if target_sym.kind() == SymKind::UnresolvedType {
                if let Some(resolved) = target_resolved
                    && resolved.id() != target_sym.id()
                    && !matches!(resolved.kind(), SymKind::UnresolvedType)
                {
                    target_sym.set_type_of(resolved.id());
                    target_sym.set_kind(resolved.kind());
                    target_sym.set_is_global(resolved.is_global());

                    if let Some(resolved_scope) = resolved.opt_scope()
                        && let Some(target_scoped) = target_sym.opt_scope()
                    {
                        let target_scope = unit.get_scope(target_scoped);
                        let resolved_scope = unit.get_scope(resolved_scope);
                        target_scope.add_parent(resolved_scope);
                        resolved_scope.add_parent(target_scope);
                    }
                }
            }

            if let Some(trait_node) = node.child_by_field(unit, LangRust::field_trait)
                && let Some(trait_ident) = trait_node.find_ident(unit)
            {
                // Only look for Trait kind - don't use SYM_KIND_TYPES which includes TypeParameter
                // If not found here, keep the existing symbol (UnresolvedType from collection)
                // and let graph phase handle cross-file resolution
                let trait_sym =
                    scopes.lookup_symbol(trait_ident.name, SymKindSet::from_kind(SymKind::Trait));

                if let Some(trait_sym) = trait_sym {
                    // Update the trait identifier's symbol to point to the resolved trait
                    trait_ident.set_symbol(trait_sym);

                    // Build parent scope relationship if target is resolved
                    if let Some(target_resolved) = target_resolved
                        && let Some(target_scope) = target_resolved.opt_scope()
                        && let Some(trait_scope) = trait_sym.opt_scope()
                    {
                        let target_scope = unit.get_scope(target_scope);
                        let trait_scope = unit.get_scope(trait_scope);
                        target_scope.add_parent(trait_scope);
                    }
                }

                // Extract type arguments from trait reference (e.g., User from `impl Repository<User>`)
                // and store them on the target symbol's nested_types for graph edge building
                // Use target_sym (always available) rather than target_resolved (may be None for cross-file)
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

            let sn = node.as_scope().unwrap();
            let target_scope = unit.get_scope(target_sym.opt_scope().unwrap());
            self.visit_scoped_named(unit, node, sn, scopes, target_scope, Some(target_sym), None);
        }
    }

    // AST: func(args) or obj.method(args) or path::func(args)
    // The function part (identifier, scoped_identifier, field_expression) is resolved
    // by visit_children through the appropriate visitor (visit_identifier, visit_scoped_identifier, etc.)
    fn visit_call_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // For simple identifiers that weren't resolved by visit_identifier,
        // try to resolve them as callable symbols
        if let Some(func_node) = node.child_by_field(unit, LangRust::field_function)
            && func_node.kind_id() == LangRust::identifier
            && let Some(ident) = func_node.as_ident()
            && ident.opt_symbol().is_none()
            && let Some(symbol) = scopes.lookup_symbol(ident.name, SYM_KIND_CALLABLE)
        {
            ident.set_symbol(symbol);
        }
    }

    // AST: enum Name { Variant1, Variant2, ... }
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

    // AST: macro_rules! name { ... }
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

    // AST: macro!(args) or macro![args]
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

    fn visit_generic_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    // AST: const NAME: Type = value;
    fn visit_const_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(const_sym) = node.ident_symbol_by_field(unit, LangRust::field_name)
            && let Some(const_ty) = node.child_by_field(unit, LangRust::field_type)
            && let Some(ty) = infer_type(unit, scopes, &const_ty)
        {
            const_sym.set_type_of(ty.id());
        }
    }

    // AST: static [mut] NAME: Type = value;
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

    // AST: type Alias = ConcreteType;
    fn visit_type_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(name_sym) = node.ident_symbol(unit)
            && let Some(type_sym) = node.ident_symbol_by_field(unit, LangRust::field_type)
        {
            name_sym.set_type_of(type_sym.id());
        }
    }

    // AST: [ElementType; length] or [ElementType]
    fn visit_array_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let sn = node.as_scope().unwrap();
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(ident) = sn.opt_ident()
            && let Some(symbol) =
                scopes.lookup_symbol(ident.name, SymKindSet::from_kind(SymKind::CompositeType))
            && symbol.nested_types().is_none()
        {
            if let Some(array_type_sym) = node.ident_symbol_by_field(unit, LangRust::field_element)
            {
                symbol.add_nested_type(array_type_sym.id());
            }
        }
    }

    // AST: (Type1, Type2, Type3)
    fn visit_tuple_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let sn = node.as_scope().unwrap();
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(tuple_ident) = sn.opt_ident()
            && let Some(tuple_symbol) = scopes.lookup_symbol(
                tuple_ident.name,
                SymKindSet::from_kind(SymKind::CompositeType),
            )
            && tuple_symbol.nested_types().is_none()
        {
            for type_ident in node.collect_idents(unit) {
                if let Some(type_sym) = type_ident.opt_symbol() {
                    tuple_symbol.add_nested_type(type_sym.id());
                }
            }
        }
    }

    // AST: dyn Trait or impl Trait
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

    // AST: enum Variant or enum Variant { fields } or enum Variant(types)
    fn visit_enum_variant(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_struct_item(unit, node, scopes, namespace, parent);
    }

    // AST: object.field or tuple.0 (field access expression)
    fn visit_field_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(value_sym) = node.ident_symbol_by_field(unit, LangRust::field_value)
            && let Some(field_node) = node.child_by_field(unit, LangRust::field_field)
        {
            // numeric field access (tuple indexing like tuple.0)
            if field_node.kind_id() == LangRust::integer_literal {
                if let Some(field_sym) = field_node.ident_symbol(unit) {
                    field_sym.set_field_of(value_sym.id());

                    // try to resolve element type from tuple's nested_types
                    if let Some(text) = field_node.as_text().map(|s| s.text()) {
                        if let Ok(index) = text.parse::<usize>() {
                            if let Some(value_type_id) = value_sym.type_of()
                                && let Some(value_type) = unit.opt_get_symbol(value_type_id)
                                && let Some(nested) = value_type.nested_types()
                                && index < nested.len()
                            {
                                field_sym.set_type_of(nested[index]);
                            }
                        }
                    }
                }
            }
            // named field access (struct.field)
            else if let Some(field_ident) = field_node.find_ident(unit) {
                if let Some(field_sym) = field_ident.opt_symbol() {
                    field_sym.set_field_of(value_sym.id());
                }
            }
        }
    }

    // AST: path::to::identifier or module::item
    // For nested paths like `crate_b::utils::helper`:
    //   - path child is `scoped_identifier` (crate_b::utils)
    //   - name child is `identifier` (helper)
    // After visit_children, the path's name (utils) should have its symbol set.
    fn visit_scoped_identifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        let name_ident = node.ident_by_field(unit, LangRust::field_name);
        let path_node = node.child_by_field(unit, LangRust::field_path);

        if let Some(name_ident) = name_ident
            && let Some(path_node) = path_node
        {
            // Get the path symbol - depends on whether path is a simple identifier or nested scoped_identifier
            let path_sym = if path_node.kind_id() == LangRust::scoped_identifier {
                // For nested scoped_identifier, the symbol is on the path's "name" field
                // (which was resolved by the recursive visit_children call)
                path_node
                    .ident_by_field(unit, LangRust::field_name)
                    .and_then(|i| i.opt_symbol())
            } else if path_node.kind_id() == LangRust::identifier {
                // For simple identifier path, get or resolve it
                let path_ident = path_node.as_ident();
                if let Some(ident) = path_ident {
                    if let Some(sym) = ident.opt_symbol() {
                        Some(sym)
                    } else {
                        // Path doesn't have a symbol yet - look it up
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
                if let Some(name_sym) = scopes.lookup_member_symbol(path_sym, name_ident.name, None)
                {
                    name_ident.set_symbol(name_sym);
                }
            }
        }
    }

    // AST: fn foo(param: Type, ...)
    fn visit_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        if let Some(type_node) = node.child_by_field(unit, LangRust::field_type)
            && let Some(pattern) = node.child_by_field_recursive(unit, LangRust::field_pattern)
        {
            if let Some(type_sym) = infer_type(unit, scopes, &type_node) {
                bind_pattern_types(unit, scopes, &pattern, type_sym);
            }
        }
    }

    // AST: fn method(&self) or fn method(&mut self) or fn method(self)
    // The self parameter has implicit type of Self, which was bound to the struct during impl visit
    fn visit_self_param(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // Look up the "self" TypeAlias symbol which was defined in the impl scope
        // and has type_of pointing to the struct
        if let Some(self_sym) =
            scopes.lookup_symbol("self", SymKindSet::from_kind(SymKind::TypeAlias))
        {
            // Find the "self" identifier child and set its symbol
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

    // AST: path::to::type or module::item
    fn visit_scoped_type_identifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_scoped_identifier(unit, node, scopes, namespace, parent);
    }

    // AST: let name: Type = value; or let name = value;
    fn visit_let_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // Handle explicit type annotation: let x: Type = value;
        // Use infer_type to handle composite types (tuples, arrays, etc.)
        if let Some(type_node) = node.child_by_field(unit, LangRust::field_type)
            && let Some(pattern) = node.child_by_field_recursive(unit, LangRust::field_pattern)
        {
            if let Some(type_sym) = infer_type(unit, scopes, &type_node) {
                bind_pattern_types(unit, scopes, &pattern, type_sym);
                return;
            }
            // Fallback to direct ident symbol lookup for simple types
            if let Some(type_sym) = type_node.ident_symbol(unit) {
                bind_pattern_types(unit, scopes, &pattern, type_sym);
                return;
            }
        }

        // Handle type inference from value: let x = value;
        if let Some(value_node) = node.child_by_field(unit, LangRust::field_value)
            && let Some(pattern) = node.child_by_field_recursive(unit, LangRust::field_pattern)
            && let Some(type_sym) = infer_type(unit, scopes, &value_node)
        {
            bind_pattern_types(unit, scopes, &pattern, type_sym);
            return;
        }
    }

    // AST: Pattern { field1, field2 } or TupleVariant(a, b, c)
    fn visit_tuple_struct_pattern(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        let type_node = node.child_by_field(unit, LangRust::field_type);
        if let Some(type_node) = type_node
            && let Some(type_ident) = type_node.find_ident(unit)
            // type_sym is the struct type
            && let Some(type_sym) = type_ident.opt_symbol()
        {
            if type_sym.nested_types().is_some() {
                for (i, child) in node.collect_idents(unit).into_iter().enumerate() {
                    if let Some(child_sym) = child.opt_symbol()
                        && let Some(nested_types) = type_sym.nested_types()
                        && i >= 2
                        && i < nested_types.len()
                    {
                        child_sym.set_type_of(nested_types[i]);
                    }
                }
            }
        }
    }

    // AST: StructName { field1: value1, field2: value2 }
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

    // AST: match scrutinee { pattern1 => expr1, pattern2 => expr2 }
    fn visit_match_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    // AST: match arm body or block in match expression
    fn visit_match_block(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_block(unit, node, scopes, namespace, parent);
    }
}

/// Recursively extract all nested type arguments from a type node.
/// For `Result<User, Error>`, this yields [User, Error].
/// For `Vec<Vec<User>>`, this yields [Vec<User>, User] (outer Vec then inner).
fn extract_nested_types<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    type_node: &HirNode<'tcx>,
    target_sym: &Symbol,
) {
    // Check if this is a generic type node
    if type_node.kind_id() == LangRust::generic_type {
        // Get the type_arguments child
        if let Some(type_args) = type_node.child_by_field(unit, LangRust::field_type_arguments) {
            for &child_id in type_args.child_ids() {
                let child = unit.hir_node(child_id);
                if child.is_trivia() || child.kind_id() == LangRust::lifetime {
                    continue;
                }
                // Infer the type of each type argument
                if let Some(type_arg_sym) = infer_type(unit, scopes, &child) {
                    target_sym.add_nested_type(type_arg_sym.id());
                }
                // Recursively extract from nested generics
                extract_nested_types(unit, scopes, &child, target_sym);
            }
        }
    }
    // Handle scoped type identifiers (e.g., std::result::Result<T, E>)
    else if type_node.kind_id() == LangRust::scoped_type_identifier {
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
