use llmcc_core::ResolveOptions;
use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{SYM_KIND_ALL, SYM_KIND_CALLABLE, SymKindSet, Symbol};
use llmcc_resolver::BindCtxt;

use crate::LangGo;

pub(crate) fn bind_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    globals: &'tcx Scope<'tcx>,
    options: &ResolveOptions,
) {
    let _ = options;
    let mut ctxt = BindCtxt::new(unit, globals);
    visit_node(unit, node, &mut ctxt, globals, None);
}

fn visit_children<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    ctxt: &mut BindCtxt<'tcx>,
    namespace: &'tcx Scope<'tcx>,
    parent: Option<&'tcx Symbol>,
) {
    for child in node.children(&unit) {
        visit_node(unit, &child, ctxt, namespace, parent);
    }
}

fn visit_node<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    ctxt: &mut BindCtxt<'tcx>,
    namespace: &'tcx Scope<'tcx>,
    parent: Option<&'tcx Symbol>,
) {
    match node.kind_id() {
        LangGo::type_spec | LangGo::function_declaration | LangGo::method_declaration => {
            visit_scope(unit, node, ctxt, namespace, parent)
        }
        LangGo::call_expression => bind_call(unit, node, ctxt, namespace, parent),
        LangGo::identifier | LangGo::type_identifier => bind_identifier(node, ctxt, SYM_KIND_ALL),
        _ => visit_children(unit, node, ctxt, namespace, parent),
    }
}

fn visit_scope<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    ctxt: &mut BindCtxt<'tcx>,
    namespace: &'tcx Scope<'tcx>,
    parent: Option<&'tcx Symbol>,
) {
    let depth = ctxt.depth();
    let Some(scope_node) = node.as_scope() else {
        visit_children(unit, node, ctxt, namespace, parent);
        return;
    };
    let child_parent = scope_node.try_symbol().or(parent);

    if ctxt.push_node_scope(scope_node) {
        visit_children(unit, node, ctxt, ctxt.current(), child_parent);
        ctxt.pop_to(depth);
    } else {
        visit_children(unit, node, ctxt, namespace, child_parent);
    }
}

fn bind_call<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    ctxt: &mut BindCtxt<'tcx>,
    namespace: &'tcx Scope<'tcx>,
    parent: Option<&'tcx Symbol>,
) {
    if let Some(function_node) = node.child_by_field(&unit, LangGo::field_function) {
        visit_node(unit, &function_node, ctxt, namespace, parent);

        if let Some(ident) = function_node.query(&unit).try_first_ident()
            && ident.try_symbol().is_none()
            && let Some(symbol) = ctxt.lookup_symbol(ident.name, SYM_KIND_CALLABLE)
        {
            ident.set_symbol(symbol);
        }
    }

    for child in node.children(&unit) {
        if child.field_id() != LangGo::field_function {
            visit_node(unit, &child, ctxt, namespace, parent);
        }
    }
}

fn bind_identifier<'tcx>(node: &HirNode<'tcx>, ctxt: &BindCtxt<'tcx>, kinds: SymKindSet) {
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
