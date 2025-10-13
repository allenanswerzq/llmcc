use std::mem;

use llmcc_core::context::Context;
use llmcc_core::ir::{HirId, HirNode};
use llmcc_core::symbol::{Scope, ScopeStack, SymId};

use crate::descriptor::{
    CallDescriptor, EnumDescriptor, FunctionDescriptor, StructDescriptor, VariableDescriptor,
};
use crate::token::{AstVisitorRust, LangRust};

/// For local resolve (single file) later, we only need to trace back to the scope stack using simple name
/// If found, a symbol saves in the Scope struct in the closest stack.
/// If not found, we need to use the full qualified name to resolve in the global system table, thus we need
/// to save global and public level symbol saved in the global system table for later use.
#[derive(Debug)]
struct DeclFinder<'tcx> {
    ctx: Context<'tcx>,
    scope_stack: ScopeStack<'tcx>,
    functions: Vec<FunctionDescriptor>,
    variables: Vec<VariableDescriptor>,
    calls: Vec<CallDescriptor>,
    structs: Vec<StructDescriptor>,
    enums: Vec<EnumDescriptor>,
    function_stack: Vec<String>,
}

impl<'tcx> DeclFinder<'tcx> {
    pub fn new(ctx: Context<'tcx>, globals: &'tcx Scope<'tcx>) -> Self {
        let gcx = ctx.gcx;
        let mut scope_stack = ScopeStack::new(&gcx.arena, &gcx.interner);
        scope_stack.push(globals);
        Self {
            ctx,
            scope_stack,
            functions: Vec::new(),
            variables: Vec::new(),
            calls: Vec::new(),
            structs: Vec::new(),
            enums: Vec::new(),
            function_stack: Vec::new(),
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
                return owners.join("::".into());
            }
        }

        unreachable!()
    }

    fn take_functions(&mut self) -> Vec<FunctionDescriptor> {
        mem::take(&mut self.functions)
    }

    fn take_variables(&mut self) -> Vec<VariableDescriptor> {
        mem::take(&mut self.variables)
    }

    fn take_calls(&mut self) -> Vec<CallDescriptor> {
        mem::take(&mut self.calls)
    }

    fn take_structs(&mut self) -> Vec<StructDescriptor> {
        mem::take(&mut self.structs)
    }

    fn take_enums(&mut self) -> Vec<EnumDescriptor> {
        mem::take(&mut self.enums)
    }

    fn process_function_item(&mut self, node: &HirNode<'tcx>) -> Option<String> {
        let ident = node.child_by_field(self.ctx, LangRust::field_name);
        let Some(ident) = ident.as_ident() else {
            return None;
        };
        let symbol = self.scope_stack.find_or_insert_local(node.hir_id(), ident);

        let descriptor = FunctionDescriptor::from_hir(self.ctx, node);
        let fqn = descriptor
            .as_ref()
            .map(|desc| desc.fqn.clone())
            .unwrap_or_else(|| self.generate_fqn(&ident.name, node.hir_id()));

        symbol.set_fqn(fqn.clone(), self.ctx.interner());

        if let Some(mut desc) = descriptor {
            desc.set_fqn(fqn.clone());
            self.functions.push(desc);
        }

        self.ctx.insert_def(node.hir_id(), symbol);
        Some(fqn)
    }

    fn process_let_declaration(&mut self, node: &HirNode<'tcx>) -> Option<SymId> {
        let ident = node.child_by_field(self.ctx, LangRust::field_pattern);
        let Some(ident) = ident.as_ident() else {
            return None;
        };
        let symbol = self.scope_stack.find_or_insert_local(node.hir_id(), ident);

        let fqn = self.generate_fqn(&ident.name, node.hir_id());
        symbol.set_fqn(fqn.clone(), self.ctx.interner());

        let variable =
            VariableDescriptor::from_let(self.ctx, node, ident.name.clone(), fqn.clone());
        self.variables.push(variable);

        self.ctx.insert_def(node.hir_id(), symbol);
        Some(symbol.id)
    }

    fn process_parameter(&mut self, node: &HirNode<'tcx>) -> Option<SymId> {
        let ident = node.child_by_field(self.ctx, LangRust::field_pattern);
        let Some(ident) = ident.as_ident() else {
            return None;
        };
        let symbol = self.scope_stack.find_or_insert_local(node.hir_id(), ident);

        let fqn = self.generate_fqn(&ident.name, node.hir_id());
        symbol.set_fqn(fqn.clone(), self.ctx.interner());

        self.ctx.insert_def(node.hir_id(), symbol);
        Some(symbol.id)
    }

    fn process_const_like(&mut self, node: &HirNode<'tcx>, kind: &'static str) {
        let ident = node.child_by_field(self.ctx, LangRust::field_name);
        let Some(ident) = ident.as_ident() else {
            return;
        };

        let symbol = self.scope_stack.find_or_insert_global(node.hir_id(), ident);

        let fqn = self.generate_fqn(&ident.name, node.hir_id());
        symbol.set_fqn(fqn.clone(), self.ctx.interner());

        let variable = match kind {
            "const_item" => {
                VariableDescriptor::from_const_item(self.ctx, node, ident.name.clone(), fqn.clone())
            }
            "static_item" => VariableDescriptor::from_static_item(
                self.ctx,
                node,
                ident.name.clone(),
                fqn.clone(),
            ),
            _ => return,
        };

        self.variables.push(variable);
        self.ctx.insert_def(node.hir_id(), symbol);
    }

    fn process_struct_item(&mut self, node: &HirNode<'tcx>) {
        let ident = node.child_by_field(self.ctx, LangRust::field_name);
        let Some(ident) = ident.as_ident() else {
            return;
        };

        let symbol = self.scope_stack.find_or_insert_global(node.hir_id(), ident);

        let fqn = self.generate_fqn(&ident.name, node.hir_id());
        symbol.set_fqn(fqn.clone(), self.ctx.interner());

        if let Some(desc) = StructDescriptor::from_struct(self.ctx, node, fqn.clone()) {
            self.structs.push(desc);
        }

        self.ctx.insert_def(node.hir_id(), symbol);
    }

    fn process_enum_item(&mut self, node: &HirNode<'tcx>) {
        let ident = node.child_by_field(self.ctx, LangRust::field_name);
        let Some(ident) = ident.as_ident() else {
            return;
        };

        let symbol = self.scope_stack.find_or_insert_global(node.hir_id(), ident);

        let fqn = self.generate_fqn(&ident.name, node.hir_id());
        symbol.set_fqn(fqn.clone(), self.ctx.interner());

        if let Some(desc) = EnumDescriptor::from_enum(self.ctx, node, fqn.clone()) {
            self.enums.push(desc);
        }

        self.ctx.insert_def(node.hir_id(), symbol);
    }

    fn visit_children_new_scope(&mut self, node: &HirNode<'tcx>) {
        let depth = self.scope_stack.depth();
        let scope = self.ctx.alloc_scope(node.hir_id());
        self.scope_stack.push(scope);
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
        let fqn = self.process_function_item(&node);
        if let Some(fqn) = fqn {
            self.function_stack.push(fqn);
            self.visit_children_new_scope(&node);
            self.function_stack.pop();
        } else {
            self.visit_children_new_scope(&node);
        }
    }

    fn visit_let_declaration(&mut self, node: HirNode<'tcx>) {
        self.process_let_declaration(&node);
        self.visit_children(&node);
    }

    fn visit_block(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }

    fn visit_parameter(&mut self, node: HirNode<'tcx>) {
        self.process_parameter(&node);
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

    fn visit_call_expression(&mut self, node: HirNode<'tcx>) {
        let enclosing = self.function_stack.last().cloned();
        let desc = CallDescriptor::from_call(self.ctx, &node, enclosing);
        self.calls.push(desc);
        self.visit_children(&node);
    }

    fn visit_const_item(&mut self, node: HirNode<'tcx>) {
        let kind = node.inner_ts_node().kind();
        self.process_const_like(&node, kind);
    }

    fn visit_static_item(&mut self, node: HirNode<'tcx>) {
        let kind = node.inner_ts_node().kind();
        self.process_const_like(&node, kind);
    }

    fn visit_struct_item(&mut self, node: HirNode<'tcx>) {
        self.process_struct_item(&node);
    }

    fn visit_enum_item(&mut self, node: HirNode<'tcx>) {
        self.process_enum_item(&node);
    }

    fn visit_unknown(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }
}

pub struct CollectionResult {
    pub functions: Vec<FunctionDescriptor>,
    pub variables: Vec<VariableDescriptor>,
    pub calls: Vec<CallDescriptor>,
    pub structs: Vec<StructDescriptor>,
    pub enums: Vec<EnumDescriptor>,
}

pub fn collect_symbols<'tcx>(
    root: HirId,
    ctx: Context<'tcx>,
    globals: &'tcx Scope<'tcx>,
) -> CollectionResult {
    let node = ctx.hir_node(root);
    let mut decl_finder = DeclFinder::new(ctx, globals);
    decl_finder.visit_node(node);
    CollectionResult {
        functions: decl_finder.take_functions(),
        variables: decl_finder.take_variables(),
        calls: decl_finder.take_calls(),
        structs: decl_finder.take_structs(),
        enums: decl_finder.take_enums(),
    }
}
