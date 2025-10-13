use llmcc_core::context::Context;
use llmcc_core::ir::{HirId, HirNode};
use llmcc_core::symbol::{Scope, ScopeStack};

use crate::token::AstVisitorRust;

#[derive(Debug)]
struct SymbolBinder<'tcx> {
    ctx: Context<'tcx>,
    scope_stack: ScopeStack<'tcx>,
}

impl<'tcx> SymbolBinder<'tcx> {
    pub fn new(ctx: Context<'tcx>, global_scope: &'tcx Scope<'tcx>) -> Self {
        let gcx = ctx.gcx;
        let mut scope_stack = ScopeStack::new(&gcx.arena, &gcx.interner);
        scope_stack.push(global_scope);
        Self { ctx, scope_stack }
    }

    fn follow_scope_deeper(&mut self, node: HirNode<'tcx>) {
        let depth = self.scope_stack.depth();
        let scope = self.ctx.alloc_scope(node.hir_id());
        self.scope_stack.push(scope);

        self.visit_children(&node);
        self.scope_stack.pop_until(depth);
    }
}

impl<'tcx> AstVisitorRust<'tcx> for SymbolBinder<'tcx> {
    fn ctx(&self) -> Context<'tcx> {
        self.ctx
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
        if self.ctx.opt_uses(id).is_none() {
            let ident = node.expect_ident();
            if let Some(def_sym) = self.scope_stack.find_ident(ident) {
                let use_sym = self.ctx.new_symbol(node.hir_id(), ident.name.clone());
                use_sym.defined.set(Some(def_sym.owner));
                self.ctx.insert_use(id, use_sym);
                return;
            }

            let ident_key = self.ctx.interner().intern(&ident.name);
            if let Some(def_sym) = self.scope_stack.lookup_global_suffix_once(&[ident_key]) {
                let use_sym = self.ctx.new_symbol(node.hir_id(), ident.name.clone());
                use_sym.defined.set(Some(def_sym.owner));
                self.ctx.insert_use(id, use_sym);
            } else {
                println!("not find ident: {}", ident.name);
            }
        }
    }
}

pub fn bind_symbols<'tcx>(root: HirId, ctx: Context<'tcx>, global_scope: &'tcx Scope<'tcx>) {
    let node = ctx.hir_node(root);
    let mut symbol_binder = SymbolBinder::new(ctx, global_scope);
    symbol_binder.visit_node(node);
}
