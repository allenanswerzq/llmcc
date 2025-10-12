use llmcc_core::context::Context;
use llmcc_core::ir::{HirId, HirNode};
use llmcc_core::symbol::{Scope, ScopeStack, SymId, SymbolRegistry};

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
struct DeclFinder<'tcx, 'reg> {
    pub ctx: Context<'tcx>,
    pub scope_stack: ScopeStack<'tcx>,
    pub registry: &'reg mut SymbolRegistry<'tcx>,
}

impl<'tcx, 'reg> DeclFinder<'tcx, 'reg> {
    pub fn new(
        ctx: Context<'tcx>,
        global_scope: &'tcx Scope<'tcx>,
        registry: &'reg mut SymbolRegistry<'tcx>,
    ) -> Self {
        let gcx = ctx.gcx;
        let mut scope_stack = ScopeStack::new(&gcx.arena, &gcx.interner);
        scope_stack.push(global_scope);
        Self {
            ctx,
            scope_stack,
            registry,
        }
    }

    fn generate_fqn(&self, name: &str, _node_id: HirId) -> String {
        let name_key = self.ctx.interner().intern(name);
        for scope in self.scope_stack.iter().rev() {
            if let Some(_) = scope.get_id(name_key) {
                let mut owners = vec![];
                for s in self.scope_stack.iter() {
                    let hir = self.ctx.hir_node(s.owner());
                    match hir {
                        HirNode::Scope(hir) => {
                            let owner_name = hir.owner_name();
                            owners.push(owner_name);
                            if s.owner() == scope.owner() {
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
        let symbol = self.scope_stack.find_or_insert_local(node.hir_id(), ident);

        let fqn = self.generate_fqn(&ident.name, node.hir_id());
        dbg!(&fqn);
        symbol.set_fqn(fqn, self.ctx.interner());

        self.registry.insert(symbol, self.ctx.interner());

        self.ctx.insert_def(node.hir_id(), symbol);
        symbol.id
    }

    /// Visit all children of a node in a new scope
    fn visit_children_new_scope(&mut self, node: &HirNode<'tcx>) {
        let depth = self.scope_stack.depth();
        let scope = self.ctx.alloc_scope(node.hir_id());
        self.scope_stack.push(scope);
        self.visit_children(&node);
        self.scope_stack.pop_until(depth);
    }
}

impl<'tcx, 'reg> AstVisitorRust<'tcx> for DeclFinder<'tcx, 'reg> {
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
struct SymbolBinder<'tcx, 'reg> {
    pub ctx: Context<'tcx>,
    pub scope_stack: ScopeStack<'tcx>,
    pub registry: &'reg SymbolRegistry<'tcx>,
}

impl<'tcx, 'reg> SymbolBinder<'tcx, 'reg> {
    pub fn new(
        ctx: Context<'tcx>,
        global_scope: &'tcx Scope<'tcx>,
        registry: &'reg SymbolRegistry<'tcx>,
    ) -> Self {
        let gcx = ctx.gcx;
        let mut scope_stack = ScopeStack::new(&gcx.arena, &gcx.interner);
        scope_stack.push(global_scope);
        Self {
            ctx,
            scope_stack,
            registry,
        }
    }

    pub fn follow_scope_deeper(&mut self, node: HirNode<'tcx>) {
        let depth = self.scope_stack.depth();
        let scope = self.ctx.alloc_scope(node.hir_id());
        self.scope_stack.push(scope);

        self.visit_children(&node);
        self.scope_stack.pop_until(depth);
    }
}

impl<'tcx, 'reg> AstVisitorRust<'tcx> for SymbolBinder<'tcx, 'reg> {
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
            if let Some(def_sym) = self.registry.lookup_suffix_once(&[ident_key]) {
                let use_sym = self.ctx.new_symbol(node.hir_id(), ident.name.clone());
                use_sym.defined.set(Some(def_sym.owner));
                self.ctx.insert_use(id, use_sym);
            } else {
                println!("not find ident: {}", ident.name);
            }
        }
    }
}

pub fn collect_symbols<'tcx>(root: HirId, ctx: Context<'tcx>, registry: &mut SymbolRegistry<'tcx>) {
    let node = ctx.hir_node(root);
    let global_scope = ctx.alloc_scope(root);
    let mut decl_finder = DeclFinder::new(ctx, global_scope, registry);
    decl_finder.visit_node(node);
}

pub fn bind_symbols<'tcx>(root: HirId, ctx: Context<'tcx>, registry: &SymbolRegistry<'tcx>) {
    let node = ctx.hir_node(root);
    let global_scope = ctx.alloc_scope(root);
    let mut symbol_binder = SymbolBinder::new(ctx, global_scope, registry);
    symbol_binder.visit_node(node);
}
