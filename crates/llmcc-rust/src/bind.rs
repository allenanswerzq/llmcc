use llmcc_core::symbol::{Scope, ScopeStack, Symbol};

use crate::descriptor::function::parse_type_expr;
use crate::descriptor::TypeExpr;
use crate::token::{AstVisitorRust, LangRust};

#[derive(Debug)]
struct SymbolBinder<'tcx> {
    unit: CompileUnit<'tcx>,
    scopes: ScopeStack<'tcx>,
}

impl<'tcx> SymbolBinder<'tcx> {
    pub fn new(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>) -> Self {
        let mut scopes = ScopeStack::new(&unit.cc.arena, &unit.cc.interner);
        scopes.push(globals);
        Self { unit, scopes }
    }

    fn enter_child_scope(
        &mut self,
        node: HirNode<'tcx>,
    ) {
        if let Some(scope) = self.unit.opt_scope(node.hir_id()) {
            if let Some(parent_simbol) = self.scopes.scoped_symbol() {
                parent_symbol.add_dependency(scope.symbol());
            }

            let depth = self.scopes.depth();
            self.scopes.push_with_symbol(scope, symbol);
            self.visit_children(&node);
            self.scopes.pop_until(depth);
        }
    }
}

impl<'tcx> AstVisitorRust<'tcx> for SymbolBinder<'tcx> {
    fn unit(&self) -> CompileUnit<'tcx> {
        self.unit
    }

    fn visit_source_file(&mut self, node: HirNode<'tcx>) {
        self.enter_child_scope(node);
    }

    fn visit_function_item(&mut self, node: HirNode<'tcx>) {
        self.enter_child_scope(node);
    }

    fn visit_let_declaration(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }

    fn visit_block(&mut self, node: HirNode<'tcx>) {
        self.enter_child_scope(node);
    }

    fn visit_parameter(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }

    fn visit_impl_item(&mut self, node: HirNode<'tcx>) {
        self.enter_child_scope(node);
    }

    fn visit_call_expression(&mut self,node:HirNode<'tcx>) {
    }

    fn visit_identifier(&mut self, node: HirNode<'tcx>) {
    }
}

pub fn bind_symbols<'tcx>(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>) {
    let root = unit.file_start_hir_id().unwrap();
    let node = unit.hir_node(root);
    let mut symbol_binder = SymbolBinder::new(unit, globals);
    symbol_binder.visit_node(node);
}
