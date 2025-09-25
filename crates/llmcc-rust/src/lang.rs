use llmcc_core::block::{BlockId, BlockKind};
use llmcc_core::context::Context;
use llmcc_core::ir::{HirId, HirIdent, HirKind, HirNode};
use llmcc_core::symbol::{Scope, ScopeStack, SymId, Symbol};

use crate::token::{AstVisitorRust, LangRust};

#[derive(Debug)]
struct DeclFinder<'tcx> {
    pub ctx: &'tcx Context<'tcx>,
    pub scope_stack: ScopeStack<'tcx>,
}

impl<'tcx> DeclFinder<'tcx> {
    pub fn new(ctx: &'tcx Context<'tcx>, globals: &'tcx Scope<'tcx>) -> Self {
        let mut scope_stack = ScopeStack::new(&ctx.arena);
        scope_stack.push_scope(globals);
        Self { ctx, scope_stack }
    }

    fn generate_mangled_name(&self, base_name: &str, node_id: HirId) -> String {
        format!("{}_{:x}", base_name, node_id.0)
    }

    fn process_declaration(&mut self, node: &HirNode<'tcx>, field_id: u16) -> SymId {
        let ident = node.expect_ident_child_by_field(&self.ctx, field_id);
        let symbol = self.scope_stack.find_or_add(node.hir_id(), ident);

        let mangled = self.generate_mangled_name(&ident.name, node.hir_id());
        *symbol.mangled_name.borrow_mut() = mangled;

        self.ctx.insert_def(node.hir_id(), symbol);
        symbol.id
    }
}

impl<'tcx> AstVisitorRust<'tcx> for DeclFinder<'tcx> {
    fn ctx(&self) -> &'tcx Context<'tcx> {
        self.ctx
    }

    fn visit_source_file(&mut self, node: HirNode<'tcx>) {
        self.visit_block(node);
    }

    fn visit_function_item(&mut self, node: HirNode<'tcx>) {
        self.process_declaration(&node, LangRust::field_name);

        let depth = self.scope_stack.depth();
        let scope = self.ctx.find_or_add_scope(node.hir_id());
        self.scope_stack.push_scope(scope);
        self.visit_children(&node);
        self.scope_stack.pop_until(depth);
    }

    fn visit_let_declaration(&mut self, node: HirNode<'tcx>) {
        self.process_declaration(&node, LangRust::field_pattern);
        self.visit_children(&node);
    }

    fn visit_block(&mut self, node: HirNode<'tcx>) {
        let depth = self.scope_stack.depth();
        let scope = self.ctx.find_or_add_scope(node.hir_id());
        self.scope_stack.push_scope(scope);
        self.visit_children(&node);
        self.scope_stack.pop_until(depth);
    }

    fn visit_parameter(&mut self, node: HirNode<'tcx>) {
        self.process_declaration(&node, LangRust::field_pattern);
        self.visit_children(&node);
    }
}

#[derive(Debug)]
struct SymbolBinder<'tcx> {
    pub ctx: &'tcx Context<'tcx>,
    pub scope_stack: ScopeStack<'tcx>,
}

impl<'tcx> SymbolBinder<'tcx> {
    pub fn new(ctx: &'tcx Context<'tcx>, globals: &'tcx Scope<'tcx>) -> Self {
        let mut scope_stack = ScopeStack::new(&ctx.arena);
        scope_stack.push_scope(globals);
        Self { ctx, scope_stack }
    }

    pub fn follow_scope_deeper(&mut self, node: HirNode<'tcx>) {
        let depth = self.scope_stack.depth();
        let scope = self.ctx.find_or_add_scope(node.hir_id());
        self.scope_stack.push_scope(scope);

        self.visit_children(&node);
        self.scope_stack.pop_until(depth);
    }
}

impl<'tcx> AstVisitorRust<'tcx> for SymbolBinder<'tcx> {
    fn ctx(&self) -> &'tcx Context<'tcx> {
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
            // if this ident does have a symbol before
            let ident = node.expect_ident();
            if let Some(def_sym) = self.scope_stack.find(ident) {
                let use_sym = self.ctx.new_symbol(node.hir_id(), ident.name.clone());
                use_sym.defined.set(Some(def_sym.owner));

                self.ctx.insert_use(id, use_sym);
            } else {
                println!("not find ident: {}", ident.name);
            }
        }
    }
}

pub fn resolve_symbols<'tcx>(root: HirId, ctx: &'tcx Context<'tcx>) {
    let node = ctx.hir_node(root);
    let globals = ctx.find_or_add_scope(root);

    let mut decl_finder = DeclFinder::new(ctx, globals);
    decl_finder.visit_node(node.clone());

    let mut symbol_binder = SymbolBinder::new(ctx, globals);
    symbol_binder.visit_node(node);
}
