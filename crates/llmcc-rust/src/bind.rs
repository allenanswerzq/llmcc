use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{Scope, ScopeStack};

use crate::token::AstVisitorRust;

#[derive(Debug)]
struct SymbolBinder<'tcx> {
    unit: CompileUnit<'tcx>,
    scope_stack: ScopeStack<'tcx>,
}

impl<'tcx> SymbolBinder<'tcx> {
    pub fn new(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>) -> Self {
        let mut scope_stack = ScopeStack::new(&unit.cc.arena, &unit.cc.interner);
        scope_stack.push(globals);
        Self { unit, scope_stack }
    }

    fn follow_scope_deeper(&mut self, node: HirNode<'tcx>) {
        let depth = self.scope_stack.depth();
        let scope = self.unit.alloc_scope(node.hir_id());
        self.scope_stack.push(scope);

        self.visit_children(&node);
        self.scope_stack.pop_until(depth);
    }
}

impl<'tcx> AstVisitorRust<'tcx> for SymbolBinder<'tcx> {
    fn unit(&self) -> CompileUnit<'tcx> {
        self.unit
    }

    fn visit_source_file(&mut self, node: HirNode<'tcx>) {
        self.follow_scope_deeper(node);
    }

    fn visit_function_item(&mut self, node: HirNode<'tcx>) {
        self.follow_scope_deeper(node);
    }

    fn visit_let_declaration(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }

    fn visit_block(&mut self, node: HirNode<'tcx>) {
        self.follow_scope_deeper(node);
    }

    fn visit_parameter(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }

    fn visit_identifier(&mut self, node: HirNode<'tcx>) {
        let id = node.hir_id();
        if self.unit.opt_uses(id).is_none() {
            let ident = node.expect_ident();
            if let Some(def_sym) = self.scope_stack.find_ident(ident) {
                let use_sym = self.unit.new_symbol(node.hir_id(), ident.name.clone());
                use_sym.defined.set(Some(def_sym.owner()));
                self.unit.insert_use(id, use_sym);
                return;
            }

            let ident_key = self.unit.interner().intern(&ident.name);
            if let Some(def_sym) = self.scope_stack.find_global_suffix_once(&[ident_key]) {
                let use_sym = self.unit.new_symbol(node.hir_id(), ident.name.clone());
                use_sym.defined.set(Some(def_sym.owner()));
                self.unit.insert_use(id, use_sym);
            } else {
                println!("not find ident: {}", ident.name);
            }
        }
    }
}

pub fn bind_symbols<'tcx>(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>) {
    let root = unit.file_start_hir_id().unwrap();
    let node = unit.hir_node(root);
    let mut symbol_binder = SymbolBinder::new(unit, globals);
    symbol_binder.visit_node(node);
}
