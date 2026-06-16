use llmcc_core::ResolveOptions;
use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirIdent, HirKind, HirNode, HirScope};
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{
    SYM_KIND_ALL, SYM_KIND_CALLABLE, SYM_KIND_TYPES, SymKind, SymKindSet, Symbol,
};
use llmcc_resolver::BindCtxt;

use crate::infer::infer_type;
use crate::token::AstVisitorCpp;
use crate::token::LangCpp;

/// Resolves one C/C++ HIR unit against symbols published during collection.
///
/// Binding is intentionally local to the unit. The pass annotates identifiers
/// and collected symbols with resolved links, while global scope publication
/// remains the collector's responsibility.
#[derive(Debug)]
struct Binder;

impl Binder {
    /// Enter a collected semantic scope, visit its body, then restore depth.
    fn visit_collected_scope<'tcx>(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scope_node: &'tcx HirScope<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let depth = ctxt.depth();
        let child_parent = scope_node.try_symbol().or(parent);

        if ctxt.push_node_scope(scope_node) {
            self.visit_children(unit, node, ctxt, ctxt.current(), child_parent);
            ctxt.pop_to(depth);
        } else {
            self.visit_children(unit, node, ctxt, ctxt.current(), child_parent);
        }
    }

    fn visit_scope_or_children<'tcx>(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let Some(scope_node) = node.as_scope() else {
            self.visit_children(unit, node, ctxt, namespace, parent);
            return;
        };

        self.visit_collected_scope(unit, node, scope_node, ctxt, parent);
    }

    /// Type scopes bind the synthetic `this` variable before visiting members.
    fn visit_type_scope<'tcx>(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scope_node: &'tcx HirScope<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let depth = ctxt.depth();
        let child_parent = scope_node.try_symbol().or(parent);

        if ctxt.push_node_scope(scope_node) {
            Self::bind_this_variable(ctxt, scope_node);
            self.visit_children(unit, node, ctxt, ctxt.current(), child_parent);
            ctxt.pop_to(depth);
        } else {
            self.visit_children(unit, node, ctxt, ctxt.current(), child_parent);
        }
    }

    /// Anonymous lexical scopes are optional in HIR; missing scopes fall back to normal traversal.
    fn visit_lexical_scope<'tcx>(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let depth = ctxt.depth();
        if let Some(scope_node) = node.as_scope()
            && ctxt.push_node_scope(scope_node)
        {
            self.visit_children(unit, node, ctxt, ctxt.current(), parent);
            ctxt.pop_to(depth);
        } else {
            self.visit_children(unit, node, ctxt, namespace, parent);
        }
    }

    fn push_named_scope<'tcx>(
        ctxt: &mut BindCtxt<'tcx>,
        name: &str,
        kind: SymKind,
    ) -> Option<&'tcx Symbol> {
        let symbol = ctxt.lookup_symbol(name, SymKindSet::from_kind(kind))?;
        let scope_id = symbol.try_owned_scope()?;
        ctxt.push_scope(scope_id).then_some(symbol)
    }

    /// Collection attaches declaration symbols to the declarator subtree.
    fn declarator_symbol<'tcx>(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
    ) -> Option<&'tcx Symbol> {
        node.child_by_field(unit, LangCpp::field_declarator)?
            .query(unit)
            .try_first_ident()?
            .try_symbol()
    }

    fn bind_declared_type<'tcx>(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &BindCtxt<'tcx>,
        symbol: &Symbol,
    ) {
        let Some(type_node) = Self::declared_type_node(unit, node) else {
            return;
        };
        let Some(resolved_type) = infer_type(unit, ctxt, &type_node) else {
            return;
        };

        symbol.set_type_of(Self::resolve_alias_target(unit, resolved_type).id());
    }

    fn declared_type_node<'tcx>(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
    ) -> Option<HirNode<'tcx>> {
        node.child_by_field(unit, LangCpp::field_type).or_else(|| {
            node.children(unit).into_iter().find(|child| {
                matches!(
                    child.kind_id(),
                    LangCpp::type_identifier
                        | LangCpp::qualified_identifier
                        | LangCpp::template_type
                        | LangCpp::type_descriptor
                        | LangCpp::primitive_type
                        | LangCpp::sized_type_specifier
                )
            })
        })
    }

    fn resolve_alias_target<'tcx>(unit: &CompileUnit<'tcx>, symbol: &'tcx Symbol) -> &'tcx Symbol {
        let mut current = symbol;

        for _ in 0..32 {
            if current.kind() != SymKind::TypeAlias {
                return current;
            }

            let Some(target_id) = current.type_of() else {
                return current;
            };
            let Some(target) = unit.try_symbol(target_id) else {
                return current;
            };
            current = target;
        }

        current
    }

    fn bind_field_declaration<'tcx>(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &BindCtxt<'tcx>,
        owner_scope: &'tcx Scope<'tcx>,
    ) {
        let Some(field_symbol) = Self::declarator_symbol(unit, node) else {
            return;
        };

        if let Some(owner) = owner_scope.try_symbol() {
            field_symbol.set_field_of(owner.id());
        }

        Self::bind_declared_type(unit, node, ctxt, field_symbol);
    }

    fn bind_parameter_declaration<'tcx>(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &BindCtxt<'tcx>,
    ) {
        let Some(parameter_symbol) = Self::declarator_symbol(unit, node) else {
            return;
        };

        Self::bind_declared_type(unit, node, ctxt, parameter_symbol);
    }

    fn bind_variable_declaration<'tcx>(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &BindCtxt<'tcx>,
    ) {
        let Some(symbol) = node
            .query(unit)
            .identifiers()
            .into_iter()
            .filter_map(|ident| ident.try_symbol())
            .find(|symbol| symbol.kind() == SymKind::Variable)
        else {
            return;
        };

        Self::bind_declared_type(unit, node, ctxt, symbol);
    }

    fn bind_variable_identifier_type<'tcx>(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &BindCtxt<'tcx>,
    ) {
        let Some(ident) = node.as_ident() else {
            return;
        };
        let Some(symbol) = ident.try_symbol() else {
            return;
        };
        if symbol.kind() != SymKind::Variable || symbol.type_of().is_some() {
            return;
        }

        let Some(declaration) = Self::enclosing_declaration(unit, node) else {
            return;
        };
        Self::bind_declared_type(unit, &declaration, ctxt, symbol);
    }

    fn actual_value_type<'tcx>(
        unit: &CompileUnit<'tcx>,
        ctxt: &BindCtxt<'tcx>,
        symbol: &'tcx Symbol,
    ) -> Option<&'tcx Symbol> {
        if let Some(type_id) = symbol.type_of()
            && let Some(type_symbol) = unit.try_symbol(type_id)
        {
            return Some(Self::resolve_alias_target(unit, type_symbol));
        }

        if symbol.kind() != SymKind::Variable {
            return Some(symbol);
        }

        let owner = unit.try_hir_node(symbol.owner())?;
        let type_node = Self::declared_type_node(unit, &owner)?;
        let type_symbol = Self::resolve_alias_target(unit, infer_type(unit, ctxt, &type_node)?);
        symbol.set_type_of(type_symbol.id());
        Some(type_symbol)
    }

    fn enclosing_declaration<'tcx>(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
    ) -> Option<HirNode<'tcx>> {
        let mut current = node.parent().and_then(|id| unit.try_hir_node(id));
        while let Some(node) = current {
            if node.kind_id() == LangCpp::declaration {
                return Some(node);
            }
            if matches!(
                node.kind_id(),
                LangCpp::function_definition
                    | LangCpp::field_declaration
                    | LangCpp::parameter_declaration
            ) {
                return None;
            }
            current = node.parent().and_then(|id| unit.try_hir_node(id));
        }
        None
    }

    fn bind_alias_target<'tcx>(
        unit: &CompileUnit<'tcx>,
        ctxt: &BindCtxt<'tcx>,
        alias_symbol: &'tcx Symbol,
        target_node: &HirNode<'tcx>,
    ) {
        let Some(target_symbol) = infer_type(unit, ctxt, target_node) else {
            return;
        };

        let target_symbol = Self::resolve_alias_target(unit, target_symbol);
        alias_symbol.set_type_of(target_symbol.id());
        if let Some(scope_id) = target_symbol.try_owned_scope() {
            alias_symbol.set_owned_scope(scope_id);
        }
    }

    fn bind_typedef_target<'tcx>(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &BindCtxt<'tcx>,
    ) {
        let Some(alias_symbol) = Self::declarator_symbol(unit, node) else {
            return;
        };
        let Some(target_node) = node.child_by_field(unit, LangCpp::field_type) else {
            return;
        };

        Self::bind_alias_target(unit, ctxt, alias_symbol, &target_node);
    }

    fn bind_alias_declaration_target<'tcx>(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &BindCtxt<'tcx>,
    ) {
        let Some(alias_ident) = node.query(unit).try_ident_with_field(LangCpp::field_name) else {
            return;
        };
        let Some(alias_symbol) = alias_ident.try_symbol().or_else(|| {
            ctxt.lookup_symbol(alias_ident.name, SymKindSet::from_kind(SymKind::TypeAlias))
        }) else {
            return;
        };
        let Some(target_node) = node.child_by_field(unit, LangCpp::field_type) else {
            return;
        };

        Self::bind_alias_target(unit, ctxt, alias_symbol, &target_node);
    }

    /// The collector creates `this`; binding attaches the current class type.
    fn bind_this_variable<'tcx>(ctxt: &BindCtxt<'tcx>, scope_node: &'tcx HirScope<'tcx>) {
        let Some(type_symbol) = scope_node.try_symbol() else {
            return;
        };
        let Some(this_symbol) =
            ctxt.lookup_symbol("this", SymKindSet::from_kind(SymKind::Variable))
        else {
            return;
        };

        this_symbol.set_type_of(type_symbol.id());
    }

    /// Class inheritance currently stores the primary base as `type_of`.
    fn bind_base_classes<'tcx>(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &BindCtxt<'tcx>,
        type_symbol: &Symbol,
    ) {
        for child_id in node.child_ids() {
            let child = unit.hir_node(*child_id);
            if child.kind_id() == LangCpp::base_class_clause
                && let Some(base_symbol) = Self::base_class_symbol(unit, &child, ctxt)
            {
                type_symbol.set_type_of(Self::resolve_alias_target(unit, base_symbol).id());
            }
        }
    }

    /// Base clauses are unfielded; use the first child that infers to a type.
    fn base_class_symbol<'tcx>(
        unit: &CompileUnit<'tcx>,
        base_clause: &HirNode<'tcx>,
        ctxt: &BindCtxt<'tcx>,
    ) -> Option<&'tcx Symbol> {
        for child in base_clause.children(unit) {
            if let Some(symbol) = infer_type(unit, ctxt, &child) {
                return Some(symbol);
            }
        }

        None
    }

    fn resolve_identifier<'tcx>(
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        kinds: SymKindSet,
    ) {
        let Some(ident) = node.as_ident() else {
            return;
        };
        if let Some(existing) = ident.try_symbol()
            && existing.kind().is_resolved()
        {
            return;
        }

        if let Some(symbol) = ctxt.lookup_symbol(ident.name, kinds) {
            ident.set_symbol(symbol);
        }
    }

    fn resolve_global_identifier<'tcx>(
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        kinds: SymKindSet,
    ) {
        let Some(ident) = node.as_ident() else {
            return;
        };
        if let Some(symbol) = ctxt.lookup_global(ident.name, kinds) {
            ident.set_symbol(symbol);
        }
    }

    fn callable_kinds() -> SymKindSet {
        SYM_KIND_CALLABLE.with(SymKind::Method)
    }

    fn argument_count<'tcx>(unit: &CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<usize> {
        Some(
            node.child_by_field(unit, LangCpp::field_arguments)?
                .children(unit)
                .into_iter()
                .filter(|child| !child.is_trivia() && child.kind() != HirKind::Text)
                .count(),
        )
    }

    fn argument_nodes<'tcx>(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
    ) -> Option<Vec<HirNode<'tcx>>> {
        Some(
            node.child_by_field(unit, LangCpp::field_arguments)?
                .children(unit)
                .into_iter()
                .filter(|child| !child.is_trivia() && child.kind() != HirKind::Text)
                .collect(),
        )
    }

    fn call_argument_types<'tcx>(
        unit: &CompileUnit<'tcx>,
        ctxt: &BindCtxt<'tcx>,
        node: &HirNode<'tcx>,
    ) -> Option<Vec<&'tcx Symbol>> {
        Self::argument_nodes(unit, node)?
            .iter()
            .map(|argument| Self::call_argument_type(unit, ctxt, argument))
            .collect::<Option<Vec<_>>>()
    }

    fn call_argument_type<'tcx>(
        unit: &CompileUnit<'tcx>,
        ctxt: &BindCtxt<'tcx>,
        node: &HirNode<'tcx>,
    ) -> Option<&'tcx Symbol> {
        if node.kind_id() == LangCpp::number_literal {
            let text = unit.hir_text(node);
            let primitive = if text.contains('.')
                || text.contains('e')
                || text.contains('E')
                || text.ends_with('f')
                || text.ends_with('F')
            {
                "double"
            } else {
                "int"
            };
            return ctxt.lookup_global(primitive, SymKindSet::from_kind(SymKind::Primitive));
        }

        infer_type(unit, ctxt, node).map(|symbol| Self::resolve_alias_target(unit, symbol))
    }

    fn callable_parameter_count<'tcx>(unit: &CompileUnit<'tcx>, symbol: &Symbol) -> Option<usize> {
        let node = unit.try_hir_node(symbol.owner())?;
        let declarator = node.child_by_field(unit, LangCpp::field_declarator)?;
        let parameter_list = Self::find_parameter_list(unit, &declarator)?;
        Some(
            parameter_list
                .children(unit)
                .into_iter()
                .filter(|child| {
                    matches!(
                        child.kind_id(),
                        LangCpp::parameter_declaration
                            | LangCpp::optional_parameter_declaration
                            | LangCpp::variadic_parameter_declaration
                            | LangCpp::explicit_object_parameter_declaration
                    )
                })
                .count(),
        )
    }

    fn callable_parameter_types<'tcx>(
        unit: &CompileUnit<'tcx>,
        ctxt: &BindCtxt<'tcx>,
        symbol: &Symbol,
    ) -> Option<Vec<&'tcx Symbol>> {
        let node = unit.try_hir_node(symbol.owner())?;
        let declarator = node.child_by_field(unit, LangCpp::field_declarator)?;
        let parameter_list = Self::find_parameter_list(unit, &declarator)?;
        parameter_list
            .children(unit)
            .into_iter()
            .filter(|child| {
                matches!(
                    child.kind_id(),
                    LangCpp::parameter_declaration
                        | LangCpp::optional_parameter_declaration
                        | LangCpp::variadic_parameter_declaration
                        | LangCpp::explicit_object_parameter_declaration
                )
            })
            .map(|parameter| {
                let type_node = Self::declared_type_node(unit, &parameter)?;
                infer_type(unit, ctxt, &type_node)
                    .map(|symbol| Self::resolve_alias_target(unit, symbol))
            })
            .collect::<Option<Vec<_>>>()
    }

    fn find_parameter_list<'tcx>(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
    ) -> Option<HirNode<'tcx>> {
        if node.kind_id() == LangCpp::parameter_list {
            return Some(*node);
        }
        for child in node.children(unit) {
            if let Some(parameter_list) = Self::find_parameter_list(unit, &child) {
                return Some(parameter_list);
            }
        }
        None
    }

    fn choose_callable_by_arity<'tcx>(
        unit: &CompileUnit<'tcx>,
        candidates: &[&'tcx Symbol],
        arg_count: Option<usize>,
    ) -> Option<&'tcx Symbol> {
        let arg_count = arg_count?;
        let mut matches = candidates
            .iter()
            .copied()
            .filter(|symbol| Self::callable_parameter_count(unit, symbol) == Some(arg_count));
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    }

    fn choose_callable_for_call<'tcx>(
        unit: &CompileUnit<'tcx>,
        ctxt: &BindCtxt<'tcx>,
        candidates: &[&'tcx Symbol],
        call_node: &HirNode<'tcx>,
    ) -> Option<&'tcx Symbol> {
        let arg_count = Self::argument_count(unit, call_node)?;
        let arity_candidates: Vec<_> = candidates
            .iter()
            .copied()
            .filter(|symbol| Self::callable_parameter_count(unit, symbol) == Some(arg_count))
            .collect();

        if let Some(argument_types) = Self::call_argument_types(unit, ctxt, call_node) {
            let mut best: Option<(&'tcx Symbol, usize)> = None;
            let mut unique_best = true;

            for (symbol, score) in arity_candidates.iter().filter_map(|symbol| {
                let parameter_types = Self::callable_parameter_types(unit, ctxt, symbol)?;
                (parameter_types.len() == argument_types.len()).then(|| {
                    let score = parameter_types
                        .iter()
                        .zip(argument_types.iter())
                        .filter(|(parameter, argument)| parameter.id() == argument.id())
                        .count();
                    (*symbol, score)
                })
            }) {
                match best {
                    None => best = Some((symbol, score)),
                    Some((_, best_score)) if score > best_score => {
                        best = Some((symbol, score));
                        unique_best = true;
                    }
                    Some((_, best_score)) if score == best_score => {
                        unique_best = false;
                    }
                    Some(_) => {}
                }
            }

            if let Some((best_symbol, best_score)) = best
                && unique_best
                && best_score > 0
            {
                return Some(best_symbol);
            }
        }

        Self::choose_callable_by_arity(unit, &arity_candidates, Some(arg_count))
    }

    fn bind_call_target<'tcx>(
        &mut self,
        unit: &CompileUnit<'tcx>,
        call_node: &HirNode<'tcx>,
        function_node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_auto(unit, function_node, ctxt, namespace, parent);

        let Some(function_ident) = function_node.query(unit).try_first_ident() else {
            return;
        };

        let Some(candidates) = ctxt.lookup_symbols(function_ident.name, Self::callable_kinds())
        else {
            return;
        };
        let Some(symbol) = Self::choose_callable_for_call(unit, ctxt, &candidates, call_node)
        else {
            return;
        };

        function_ident.set_symbol(symbol);
    }

    fn bind_member_call_target<'tcx>(
        &mut self,
        unit: &CompileUnit<'tcx>,
        call_node: &HirNode<'tcx>,
        field_node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) -> bool {
        let Some(receiver_node) = field_node.child_by_field(unit, LangCpp::field_argument) else {
            return false;
        };

        self.visit_auto(unit, &receiver_node, ctxt, namespace, parent);

        let Some(field_ident) = field_node
            .child_by_field(unit, LangCpp::field_field)
            .and_then(|node| node.query(unit).try_first_ident())
        else {
            return false;
        };

        let Some(receiver_type) = infer_type(unit, ctxt, &receiver_node)
            .and_then(|symbol| Self::actual_value_type(unit, ctxt, symbol))
        else {
            return false;
        };

        let Some(type_scope_id) = receiver_type.try_owned_scope() else {
            return false;
        };
        let type_scope = unit.scope(type_scope_id);
        let Some(candidates) = type_scope.try_lookup_symbols(
            unit.interner().intern(field_ident.name),
            llmcc_core::scope::SymbolFilter::kinds(Self::callable_kinds()),
        ) else {
            return false;
        };
        let symbol =
            Self::choose_callable_for_call(unit, ctxt, &candidates, call_node).or_else(|| {
                ctxt.lookup_member(receiver_type, field_ident.name, Self::callable_kinds())
            });

        if let Some(symbol) = symbol {
            field_ident.set_symbol(symbol);
            return true;
        }

        false
    }

    /// Leaf identifier for a qualified node.
    fn qualified_name_ident<'tcx>(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
    ) -> Option<&'tcx HirIdent<'tcx>> {
        node.query(unit)
            .try_ident_with_field(LangCpp::field_name)
            .or_else(|| node.as_ident())
    }

    /// Build a qualified path from `scope`/`name` fields instead of source text.
    fn qualified_path<'tcx>(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
    ) -> Option<Vec<&'tcx str>> {
        let mut path = Vec::new();
        if Self::push_qualified_path(unit, node, &mut path) && path.len() > 1 {
            Some(path)
        } else {
            None
        }
    }

    fn push_qualified_path<'tcx>(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        path: &mut Vec<&'tcx str>,
    ) -> bool {
        if let Some(scope_node) = node.child_by_field(unit, LangCpp::field_scope)
            && !Self::push_qualified_path(unit, &scope_node, path)
        {
            return false;
        }

        let Some(name_ident) = Self::qualified_name_ident(unit, node) else {
            return false;
        };
        path.push(name_ident.name);
        true
    }

    /// Resolve the whole qualified path and avoid lexical fallback for the leaf.
    fn bind_qualified_identifier<'tcx>(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &BindCtxt<'tcx>,
    ) -> bool {
        let Some(path) = Self::qualified_path(unit, node) else {
            return false;
        };

        if let Some(name_ident) = Self::qualified_name_ident(unit, node)
            && let Some(symbol) = ctxt.lookup_path_symbol(&path, SYM_KIND_ALL)
        {
            name_ident.set_symbol(symbol);
        }

        true
    }

    /// Resolve members through the receiver type; unresolved receivers leave the member unbound.
    fn bind_member_expression<'tcx>(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) -> bool {
        let Some(receiver_node) = node.child_by_field(unit, LangCpp::field_argument) else {
            return false;
        };

        self.visit_auto(unit, &receiver_node, ctxt, namespace, parent);

        let Some(field_ident) = node
            .child_by_field(unit, LangCpp::field_field)
            .and_then(|field_node| field_node.query(unit).try_first_ident())
        else {
            return true;
        };

        if let Some(receiver_type) = infer_type(unit, ctxt, &receiver_node)
            .and_then(|symbol| Self::actual_value_type(unit, ctxt, symbol))
            && let Some(symbol) = ctxt.lookup_member(receiver_type, field_ident.name, SYM_KIND_ALL)
        {
            field_ident.set_symbol(symbol);
        }

        true
    }
}

impl<'tcx> AstVisitorCpp<'tcx, BindCtxt<'tcx>> for Binder {
    fn visit_translation_unit(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let depth = ctxt.depth();
        let meta = unit.unit_meta();

        // Collection publishes metadata scopes first; binding enters the same
        // chain so same-name symbols prefer the current package/module/file.
        if let Some(package_name) = meta.package_name.as_deref() {
            Self::push_named_scope(ctxt, package_name, SymKind::Package);
        }

        if let Some(module_name) = meta.module_name.as_deref() {
            Self::push_named_scope(ctxt, module_name, SymKind::Module);
        }

        if let Some(file_symbol) = meta
            .file_name
            .as_deref()
            .and_then(|file_name| Self::push_named_scope(ctxt, file_name, SymKind::File))
        {
            self.visit_children(unit, node, ctxt, ctxt.current(), Some(file_symbol));
        } else {
            self.visit_children(unit, node, ctxt, ctxt.current(), None);
        }

        ctxt.pop_to(depth);
    }

    fn visit_identifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        Self::resolve_identifier(node, ctxt, SYM_KIND_ALL);
        Self::bind_variable_identifier_type(unit, node, ctxt);
    }

    fn visit_type_identifier(
        &mut self,
        _unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        Self::resolve_identifier(node, ctxt, SYM_KIND_TYPES);
    }

    fn visit_primitive_type(
        &mut self,
        _unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        Self::resolve_global_identifier(node, ctxt, SymKindSet::from_kind(SymKind::Primitive));
    }

    fn visit_field_identifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_identifier(unit, node, ctxt, namespace, parent);
    }

    fn visit_namespace_identifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_identifier(unit, node, ctxt, namespace, parent);
    }

    fn visit_namespace_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_scope_or_children(unit, node, ctxt, namespace, parent);
    }

    fn visit_class_specifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let Some(scope_node) = node.as_scope() else {
            self.visit_children(unit, node, ctxt, namespace, parent);
            return;
        };

        self.visit_type_scope(unit, node, scope_node, ctxt, parent);

        if let Some(type_symbol) = scope_node.try_symbol() {
            Self::bind_base_classes(unit, node, ctxt, type_symbol);
        }
    }

    fn visit_struct_specifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_class_specifier(unit, node, ctxt, namespace, parent);
    }

    fn visit_union_specifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_class_specifier(unit, node, ctxt, namespace, parent);
    }

    fn visit_enum_specifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_scope_or_children(unit, node, ctxt, namespace, parent);
    }

    fn visit_function_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let Some(scope_node) = node.as_scope() else {
            self.visit_children(unit, node, ctxt, namespace, parent);
            return;
        };

        self.visit_collected_scope(unit, node, scope_node, ctxt, parent);

        if let Some(function_symbol) = scope_node.try_symbol() {
            if let Some(type_node) = node.child_by_field(unit, LangCpp::field_type)
                && let Some(return_type) = infer_type(unit, ctxt, &type_node)
            {
                function_symbol.set_type_of(return_type.id());
            }
        }
    }

    fn visit_field_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, ctxt, namespace, parent);
        Self::bind_field_declaration(unit, node, ctxt, namespace);
    }

    fn visit_parameter_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, ctxt, namespace, parent);
        Self::bind_parameter_declaration(unit, node, ctxt);
    }

    fn visit_type_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, ctxt, namespace, parent);
        Self::bind_typedef_target(unit, node, ctxt);
    }

    fn visit_alias_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, ctxt, namespace, parent);
        Self::bind_alias_declaration_target(unit, node, ctxt);
    }

    fn visit_compound_statement(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_lexical_scope(unit, node, ctxt, namespace, parent);
    }

    fn visit_unknown(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, ctxt, namespace, parent);
        if node.kind_id() == LangCpp::declaration {
            Self::bind_variable_declaration(unit, node, ctxt);
        }
    }

    fn visit_qualified_identifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if Self::bind_qualified_identifier(unit, node, ctxt) {
            return;
        }

        self.visit_children(unit, node, ctxt, namespace, parent);
    }

    fn visit_call_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(func_node) = node.child_by_field(unit, LangCpp::field_function) {
            if func_node.kind_id() == LangCpp::field_expression {
                if !self.bind_member_call_target(unit, node, &func_node, ctxt, namespace, parent) {
                    self.visit_auto(unit, &func_node, ctxt, namespace, parent);
                }
            } else {
                self.bind_call_target(unit, node, &func_node, ctxt, namespace, parent);
            }
        }

        if let Some(args_node) = node.child_by_field(unit, LangCpp::field_arguments) {
            self.visit_auto(unit, &args_node, ctxt, namespace, parent);
        }
    }

    fn visit_field_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if self.bind_member_expression(unit, node, ctxt, namespace, parent) {
            return;
        }

        self.visit_children(unit, node, ctxt, namespace, parent);
    }

    fn visit_template_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let depth = ctxt.depth();
        self.visit_children(unit, node, ctxt, namespace, parent);

        ctxt.pop_to(depth);
    }

    fn visit_lambda_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        ctxt: &mut BindCtxt<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_lexical_scope(unit, node, ctxt, namespace, parent);
    }
}

/// Bind identifiers and type relationships for a single C/C++ HIR unit.
pub(crate) fn bind_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    namespace: &'tcx Scope<'tcx>,
    _config: &ResolveOptions,
) {
    let mut ctxt = BindCtxt::new(unit, namespace);
    let mut binder = Binder;
    binder.visit_node(&unit, node, &mut ctxt, namespace, None);
}
