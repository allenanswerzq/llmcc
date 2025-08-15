pub mod arena;
// pub mod block;
pub mod ir;
pub mod ir_builder;
pub mod lang;
pub mod symbol;
pub mod visit;

pub use arena::IrArena;
pub use ir_builder::{build_llmcc_ir, print_llmcc_ir};
pub use lang::*;
pub use visit::*;

pub use tree_sitter::{Node, Parser, Point, Tree, TreeCursor};

// #[derive(Debug)]
// struct AstSymbolCollector<'a> {
//     context: &'a AstContext,
//     pub scope_stack: AstScopeStack,
// }

// impl<'a> AstSymbolCollector<'a> {
//     fn new(context: &'a AstContext) -> Self {
//         let mut collector = Self {
//             context,
//             scope_stack: AstScopeStack::new(),
//         };
//         context
//             .language
//             .add_builtin_symbol(&mut collector.scope_stack);
//         collector
//     }

//     fn upgrade_identifier_if_any(
//         &self,
//         ,
//         token_id: u16,
//         node_id: usize,
//         name: usize,
//     ) {
//         let upgrade_to = self.context.language.upgrade_identifier(token_id);
//         if let Some(upgrade) = upgrade_to {
//             let node = arena.get_mut(name).unwrap();
//             node.get_base_mut().kind = upgrade;
//             match upgrade {
//                 AstKind::IdentifierDef
//                 | AstKind::IdentifierFieldDef
//                 | AstKind::IdentifierTypeDef => {
//                     let symbol = node.get_symbol_mut().unwrap();
//                     // symbol.parent_scope = self.scope_stack.current_scope;
//                     symbol.defined = Some(node_id);
//                 }
//                 _ => {}
//             }
//         }
//     }

//     fn mangled_name(&self, , name: usize) {
//         // self.context.language.mangled_name(name, &self.scope_stack);
//     }

//     fn step_to_name(&self, cursor: &mut AstTreeCursor<'a>) {
//         let field_id = self.context.language.step_to_name(cursor.node());
//         let base = cursor.node().get_base().clone();
//         if let Some(field_id) = field_id {
//             let child = AstKindNode::child_by_field_id(cursor.get_arena(), &base, field_id);
//             match cursor.node() {
//                 AstKindNode::Scope(node) => {
//                     node.name = Some(child.unwrap());
//                 }
//                 AstKindNode::Internal(node) => {
//                     node.name = Some(child.unwrap());
//                 }
//                 _ => {}
//             }
//         }
//     }
// }

// impl<'a> Visitor<AstTreeCursor<'a>> for AstSymbolCollector<'a> {
//     fn visit_enter_node(&mut self, cursor: &mut AstTreeCursor<'a>) {
//         let node_id = if let AstKindNode::Scope(node) = cursor.node() {
//             Some(node.base.id)
//         } else {
//             None
//         };

//         if let Some(id) = node_id {
//             self.scope_stack.enter_scope(cursor.get_arena(), id);
//         }
//     }

//     fn visit_node(&mut self, cursor: &mut AstTreeCursor<'a>) {
//         self.step_to_name(cursor);

//         let token_id = cursor.node().get_base().token_id;
//         let node_id = cursor.node().get_base().id;
//         match cursor.node() {
//             AstKindNode::Scope(node) => {
//                 if let Some(name) = node.name {
//                     let arena = cursor.get_arena();
//                     self.upgrade_identifier_if_any(arena, token_id, node_id, name);
//                     self.mangled_name(arena, name);
//                     let symbol = arena.get_mut(name).unwrap().get_symbol_clone();
//                     self.scope_stack.add_symbol(arena, symbol.unwrap());
//                 }
//             }
//             AstKindNode::Internal(node) => {
//                 if let Some(name) = node.name {
//                     let arena = cursor.get_arena();
//                     self.upgrade_identifier_if_any(arena, token_id, node_id, name);
//                     self.mangled_name(arena, name);
//                     let symbol = arena.get_mut(name).unwrap().get_symbol_clone();
//                     self.scope_stack.add_symbol(arena, symbol.unwrap());
//                 }
//             }
//             AstKindNode::Identifier(node) => {
//                 match node.base.kind {
//                     AstKind::IdentifierDef
//                     | AstKind::IdentifierFieldDef
//                     | AstKind::IdentifierTypeDef => {
//                         let symbol = node.symbol.clone();
//                         let arena = cursor.get_arena();
//                         self.mangled_name(arena, node_id);
//                         self.scope_stack.add_symbol(arena, *symbol);
//                     }
//                     AstKind::IdentifierUse
//                     | AstKind::IdentifierTypeUse
//                     | AstKind::IdentifierFieldUse => {
//                         // Do nothing here in declaration pass
//                     }
//                     _ => unimplemented!(),
//                 }
//             }
//             _ => {} // Handle other node types
//         }
//     }

//     fn visit_leave_node(&mut self, cursor: &mut AstTreeCursor<'a>) {
//         let node_id = if let AstKindNode::Scope(node) = cursor.node() {
//             Some(node.base.id)
//         } else {
//             None
//         };

//         if let Some(_id) = node_id {
//             self.scope_stack.leave_scope(cursor.get_arena());
//         }
//     }
// }

// struct AstSymbolBinder<'a> {
//     context: &'a AstContext,
//     scope_stack: AstScopeStack,
// }

// impl<'a> AstSymbolBinder<'a> {
//     fn new(context: &'a AstContext, scope_stack: AstScopeStack) -> Self {
//         Self {
//             context,
//             scope_stack,
//         }
//     }

//     fn resolve_symbol(
//         &self,
//         ,
//         name: &AstSymbol,
//     ) -> Option<Box<AstSymbol>> {
//         self.scope_stack.lookup(arena, &name.name)
//     }
// }

// impl<'a> Visitor<AstTreeCursor<'a>> for AstSymbolBinder<'a> {
//     fn visit_enter_node(&mut self, cursor: &mut AstTreeCursor<'a>) {
//         let node_id = if let AstKindNode::Scope(node) = cursor.node() {
//             Some(node.base.id)
//         } else {
//             None
//         };

//         if let Some(id) = node_id {
//             self.scope_stack.enter_scope(cursor.get_arena(), id);
//         }
//     }

//     fn visit_node(&mut self, cursor: &mut AstTreeCursor<'a>) {
//         let symbol = cursor.node().get_symbol_clone();
//         if symbol.is_none() {
//             return;
//         }
//         let symbol = symbol.unwrap();
//         let kind = cursor.node().get_base().kind;
//         match kind {
//             AstKind::IdentifierUse => {
//                 if let Some(define) = self.resolve_symbol(cursor.get_arena(), &symbol) {
//                     if let AstKindNode::Identifier(node) = cursor.node() {
//                         node.symbol.defined = Some(define.defined.unwrap());
//                     } else {
//                         unreachable!()
//                     }
//                 } else {
//                     panic!("cannot resolve use symbol: {}", symbol)
//                 }
//             }
//             AstKind::IdentifierTypeUse => {
//                 if let Some(type_of) = self.resolve_symbol(cursor.get_arena(), &symbol) {
//                     if let AstKindNode::Identifier(node) = cursor.node() {
//                         node.symbol.type_of = Some(type_of);
//                     } else {
//                         unreachable!()
//                     }
//                 } else {
//                     panic!("cannot resolve tyep use symbol: {}", symbol)
//                 }
//             }
//             AstKind::IdentifierFieldUse => {
//                 if let Some(field_of) = self.resolve_symbol(cursor.get_arena(), &symbol) {
//                     if let AstKindNode::Identifier(node) = cursor.node() {
//                         node.symbol.field_of = Some(field_of);
//                     } else {
//                         unreachable!()
//                     }
//                 } else {
//                     panic!("cannot resolve field use symbol: {}", symbol)
//                 }
//             }
//             _ => {}
//         }
//     }

//     fn visit_leave_node(&mut self, cursor: &mut AstTreeCursor<'a>) {
//         let node_id = if let AstKindNode::Scope(node) = cursor.node() {
//             Some(node.base.id)
//         } else {
//             None
//         };

//         if let Some(_id) = node_id {
//             self.scope_stack.leave_scope(cursor.get_arena());
//         }
//     }
// }

// pub fn collect_llmcc_ast(
//     _tree: &AstTree,
//     context: &AstContext,
//     arena: AstArenaShare<AstKindNode>,
// ) -> AstScopeStack {
//     let mut arena_ref = arena.borrow_mut();
//     let mut collector = AstSymbolCollector::new(context);
//     let mut cursor = AstTreeCursor::new(&mut *arena_ref);
//     dfs(&mut cursor, &mut collector);
//     collector.scope_stack.reset_stack();
//     collector.scope_stack
// }

// pub fn bind_llmcc_ast(
//     _tree: &AstTree,
//     context: &AstContext,
//     arena: AstArenaShare<AstKindNode>,
//     scope_stack: AstScopeStack,
// ) {
//     let mut arena_ref = arena.borrow_mut();
//     let mut binder = AstSymbolBinder::new(context, scope_stack);
//     let mut cursor = AstTreeCursor::new(&mut *arena_ref);
//     dfs(&mut cursor, &mut binder);
// }
