use llmcc_core::ResolveOptions;
use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirId, HirIdent, HirNode, HirScope};
use llmcc_core::scope::{Scope, ScopeStack};
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_resolver::CollectCtxt;

use crate::LangPython;

pub(crate) fn collect_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    scope_stack: ScopeStack<'tcx>,
    _options: &ResolveOptions,
) -> &'tcx Scope<'tcx> {
    let context = unit.context();
    let unit_globals_value = Scope::new(HirId(unit.index()));
    let scope_id = unit_globals_value.id().0;
    let unit_globals = context.arena().alloc_with_id(scope_id, unit_globals_value);
    let mut ctxt = CollectCtxt::new(context, unit.index(), scope_stack, unit_globals);
    visit_node(unit, node, &mut ctxt, unit_globals, None);
    unit_globals
}

fn visit_children<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    ctxt: &mut CollectCtxt<'tcx>,
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
    ctxt: &mut CollectCtxt<'tcx>,
    namespace: &'tcx Scope<'tcx>,
    parent: Option<&'tcx Symbol>,
) {
    match node.kind_id() {
        LangPython::module => visit_children(unit, node, ctxt, namespace, parent),
        LangPython::class_definition => {
            visit_named_scope(unit, node, ctxt, SymKind::Struct, namespace, parent)
        }
        LangPython::function_definition => {
            let kind = if parent.is_some_and(|symbol| symbol.kind() == SymKind::Struct) {
                SymKind::Method
            } else {
                SymKind::Function
            };
            visit_named_scope(unit, node, ctxt, kind, namespace, parent);
        }
        _ => visit_children(unit, node, ctxt, namespace, parent),
    }
}

fn visit_named_scope<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    ctxt: &mut CollectCtxt<'tcx>,
    kind: SymKind,
    namespace: &'tcx Scope<'tcx>,
    parent: Option<&'tcx Symbol>,
) {
    let Some((scope_node, ident)) = scope_and_name(unit, node) else {
        visit_children(unit, node, ctxt, namespace, parent);
        return;
    };

    let Some(symbol) = ctxt.declare(ident.name, node, kind) else {
        visit_children(unit, node, ctxt, namespace, parent);
        return;
    };

    ident.set_symbol(symbol);
    scope_node.set_ident(ident);

    let depth = ctxt.depth();
    let scope = ctxt.push_symbol_scope(node, Some(symbol));
    scope_node.set_scope(scope);
    visit_children(unit, node, ctxt, scope, Some(symbol));
    ctxt.pop_to(depth);
}

fn scope_and_name<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<(&'tcx HirScope<'tcx>, &'tcx HirIdent<'tcx>)> {
    let scope = node.as_scope()?;
    let ident = node
        .query(&unit)
        .try_ident_with_field(LangPython::field_name)?;
    Some((scope, ident))
}
