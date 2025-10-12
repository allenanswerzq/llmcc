use std::collections::HashMap;
use std::marker::PhantomData;

use llmcc_core::block::{BlockId, BlockKind};
use llmcc_core::context::Context;
use llmcc_core::ir::{HirId, HirIdent, HirKind, HirNode, HirScope};
use llmcc_core::symbol::{Scope, ScopeStack, SymId, Symbol};

use crate::token::{AstVisitorRust, LangRust};

// pub enum VisibiltyEnum {
//     CRATE,
//     PUBLIC,
// }

// pub struct TypedParams {
//     /// identifier -> type
//     params: HashMap<String, String>,
// }

// /// Given a hir scope node, parse everything into a function.
// ///
// /// This node can be a tree-sitter ast:
// /// - function_item
// /// -
// ///
// /// We should easily get info after parsting:
// /// - func_name: simple, and fully qualified foo(u16,u16)->u32
// /// - visibilty: public to others etcc
// /// - return types:
// /// - all declarations
// pub struct Function<'hir> {
//     ///
//     visibilty: VisibiltyEnum,
//     /// Simple name
//     name: String,
//     /// Fully Qualified name
//     fqn_name: String,
//     /// Parameters
//     parameters: TypedParams,
//     _ph: PhantomData<&'hir ()>,
// }

// impl<'hir> Function<'hir> {
//     pub fn parse(ctx: &'hir Context<'hir>, node: &'hir HirNode<'hir>) -> Self {
//         let name = node.expect_ident_child_by_field(ctx, LangRust::field_name);
//         let params = node.child_by_field(ctx, LangRust::field_parameters);

//         Self {
//             visibilty: VisibiltyEnum::CRATE,
//             name: name.name.clone(),
//             fqn_name: name.name.clone(),
//             parameters: TypedParams {
//                 params: HashMap::new(),
//             },
//             _ph: PhantomData,
//         }
//     }
// }

// pub struct Struct {
// }

// ///
// /// - f.method()
// /// - foo()
// /// - my_mod::func(u32)
// pub struct CallExpr {
// }

#[derive(Debug)]
struct DeclFinder<'tcx> {
    pub ctx: Context<'tcx>,
    pub scope_stack: ScopeStack<'tcx>,
}

impl<'tcx> DeclFinder<'tcx> {
    pub fn new(ctx: Context<'tcx>, globals: &'tcx Scope<'tcx>) -> Self {
        let gcx = ctx.gcx;
        let mut scope_stack = ScopeStack::new(&gcx.arena);
        scope_stack.push_scope(globals);
        Self { ctx, scope_stack }
    }

    fn generate_fqn(&self, name: &str, node_id: HirId) -> String {
        for scope in self.scope_stack.scopes.iter().rev() {
            if let Some(_) = scope.find_symbol_id(name) {
                let mut owners = vec![];
                for s in self.scope_stack.scopes.iter() {
                    let hir = self.ctx.hir_node(s.owner);
                    match hir {
                        HirNode::Scope(hir) => {
                            let owner_name = hir.owner_name();
                            owners.push(owner_name);
                            if s.owner == scope.owner {
                                break;
                            }
                        }
                        HirNode::File(_node) => {}
                        _ => {}
                    }
                }
                owners.push(name.to_string());
                owners.reverse();
                return owners.join("::".into());
            }
        }

        unreachable!()
    }

    fn process_decl(&mut self, node: &HirNode<'tcx>, field_id: u16) -> SymId {
        let ident = node.child_by_field(self.ctx, field_id);
        if ident.as_ident().is_none() {
            print!("declaration without identifier: {:?}", node);
            return SymId(0);
        }
        let ident = ident.expect_ident();
        let symbol = self.scope_stack.find_or_add(node.hir_id(), ident, false);

        let fqn = self.generate_fqn(&ident.name, node.hir_id());
        dbg!(&fqn);
        *symbol.fqn_name.borrow_mut() = fqn;

        self.ctx.insert_def(node.hir_id(), symbol);
        symbol.id
    }

    /// Visit all children of a node in a new scope
    fn visit_children_new_scope(&mut self, node: &HirNode<'tcx>) {
        let depth = self.scope_stack.depth();
        let scope = self.ctx.alloc_scope(node.hir_id());
        self.scope_stack.push_scope(scope);
        self.visit_children(&node);
        self.scope_stack.pop_until(depth);
    }
}

impl<'tcx> AstVisitorRust<'tcx> for DeclFinder<'tcx> {
    fn ctx(&self) -> Context<'tcx> {
        self.ctx
    }

    fn visit_source_file(&mut self, node: HirNode<'tcx>) {
        self.visit_block(node);
    }

    fn visit_function_item(&mut self, node: HirNode<'tcx>) {
        self.process_decl(&node, LangRust::field_name);
        self.visit_children_new_scope(&node);
    }

    fn visit_let_declaration(&mut self, node: HirNode<'tcx>) {
        self.process_decl(&node, LangRust::field_pattern);
        self.visit_children(&node);
    }

    fn visit_block(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }

    fn visit_parameter(&mut self, node: HirNode<'tcx>) {
        self.process_decl(&node, LangRust::field_pattern);
        self.visit_children(&node);
    }

    fn visit_mod_item(&mut self, node: HirNode<'tcx>) {
        self.visit_children_new_scope(&node);
    }

    fn visit_impl_item(&mut self, node: HirNode<'tcx>) {
        self.visit_children_new_scope(&node);
    }

    fn visit_trait_item(&mut self, node: HirNode<'tcx>) {
        self.visit_children_new_scope(&node);
    }

    // fn visit_function_signature_item(&mut self, node: HirNode<'tcx>) {
    //     // self.visit_children_new_scope(&node);
    // }
}

#[derive(Debug)]
struct SymbolBinder<'tcx> {
    pub ctx: Context<'tcx>,
    pub scope_stack: ScopeStack<'tcx>,
}

impl<'tcx> SymbolBinder<'tcx> {
    pub fn new(ctx: Context<'tcx>, globals: &'tcx Scope<'tcx>) -> Self {
        let gcx = ctx.gcx;
        let mut scope_stack = ScopeStack::new(&gcx.arena);
        scope_stack.push_scope(globals);
        Self { ctx, scope_stack }
    }

    pub fn follow_scope_deeper(&mut self, node: HirNode<'tcx>) {
        let depth = self.scope_stack.depth();
        let scope = self.ctx.alloc_scope(node.hir_id());
        self.scope_stack.push_scope(scope);

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

pub fn resolve_symbols<'tcx>(root: HirId, ctx: Context<'tcx>) {
    let node = ctx.hir_node(root);
    let globals = ctx.alloc_scope(root);

    let mut decl_finder = DeclFinder::new(ctx, globals);
    decl_finder.visit_node(node.clone());

    let mut symbol_binder = SymbolBinder::new(ctx, globals);
    symbol_binder.visit_node(node);
}
