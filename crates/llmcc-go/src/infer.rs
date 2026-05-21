use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::symbol::{SYM_KIND_TYPES, SymKind, Symbol};
use llmcc_resolver::BinderScopes;

use crate::LangGo;

const MAX_INFER_DEPTH: u32 = 16;

pub fn infer_type<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    infer_type_impl(unit, scopes, node, 0)
}

fn infer_type_impl<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    if depth >= MAX_INFER_DEPTH {
        return None;
    }

    match node.kind_id() {
        LangGo::int_literal => get_primitive_type(scopes, "int"),
        LangGo::float_literal => get_primitive_type(scopes, "float64"),
        LangGo::interpreted_string_literal | LangGo::raw_string_literal => {
            get_primitive_type(scopes, "string")
        }
        LangGo::r#true | LangGo::r#false => get_primitive_type(scopes, "bool"),
        LangGo::nil => None,

        LangGo::type_identifier | LangGo::identifier => {
            let ident = node.find_ident(unit)?;
            if let Some(sym) = ident.opt_symbol()
                && sym.kind() != SymKind::UnresolvedType
            {
                return Some(sym);
            }
            scopes
                .lookup_symbol(ident.name, SYM_KIND_TYPES)
                .or_else(|| scopes.lookup_global(ident.name, SYM_KIND_TYPES))
                .or_else(|| ident.opt_symbol())
        }

        LangGo::qualified_type => node
            .child_by_field(unit, LangGo::field_name)
            .and_then(|name| infer_type_impl(unit, scopes, &name, depth + 1)),

        LangGo::pointer_type | LangGo::generic_type => {
            infer_from_children(unit, scopes, node, depth + 1)
        }

        LangGo::call_expression => node
            .child_by_field(unit, LangGo::field_function)
            .and_then(|func| infer_type_impl(unit, scopes, &func, depth + 1))
            .and_then(|sym| {
                if matches!(sym.kind(), SymKind::Function | SymKind::Method)
                    && let Some(type_id) = sym.type_of()
                {
                    return unit.opt_get_symbol(type_id);
                }
                Some(sym)
            }),

        LangGo::selector_expression => node
            .child_by_field(unit, LangGo::field_field)
            .and_then(|field| infer_type_impl(unit, scopes, &field, depth + 1)),

        _ => {
            if node.kind() == HirKind::Identifier {
                return node.as_ident().and_then(|ident| ident.opt_symbol());
            }
            infer_from_children(unit, scopes, node, depth + 1)
        }
    }
}

fn infer_from_children<'tcx>(
    unit: &CompileUnit<'tcx>,
    scopes: &BinderScopes<'tcx>,
    node: &HirNode<'tcx>,
    depth: u32,
) -> Option<&'tcx Symbol> {
    for child in node.children(unit) {
        if let Some(sym) = infer_type_impl(unit, scopes, &child, depth) {
            return Some(sym);
        }
    }
    None
}

fn get_primitive_type<'tcx>(scopes: &BinderScopes<'tcx>, name: &str) -> Option<&'tcx Symbol> {
    scopes.lookup_global(name, SYM_KIND_TYPES)
}
