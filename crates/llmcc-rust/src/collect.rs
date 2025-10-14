use std::mem;

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{Scope, ScopeStack, SymId, Symbol};

use crate::descriptor::{
    CallDescriptor, EnumDescriptor, FunctionDescriptor, StructDescriptor, TypeExpr,
    VariableDescriptor,
};
use crate::descriptor::function::parse_type_expr;
use crate::token::{AstVisitorRust, LangRust};

#[derive(Debug)]
struct DeclFinder<'tcx> {
    unit: CompileUnit<'tcx>,
    scopes: ScopeStack<'tcx>,
    functions: Vec<FunctionDescriptor>,
    variables: Vec<VariableDescriptor>,
    calls: Vec<CallDescriptor>,
    structs: Vec<StructDescriptor>,
    enums: Vec<EnumDescriptor>,
}

impl<'tcx> DeclFinder<'tcx> {
    pub fn new(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>) -> Self {
        let mut scopes = ScopeStack::new(&unit.cc.arena, &unit.cc.interner);
        // TODO: make create a new symbol assoicate unit file name
        scopes.push_with_symbol(globals, None);
        Self {
            unit,
            scopes,
            functions: Vec::new(),
            variables: Vec::new(),
            calls: Vec::new(),
            structs: Vec::new(),
            enums: Vec::new(),
        }
    }

    fn parent_symbol(&self) -> Option<&'tcx Symbol> {
        self.scopes.scoped_symbol()
    }

    fn scoped_fqn(&self, _node: &HirNode<'tcx>, name: &str) -> String {
        if let Some(parent) = self.parent_symbol() {
            let parent_fqn = parent.fqn_name.borrow();
            if parent_fqn.is_empty() {
                name.to_string()
            } else {
                format!("{}::{}", parent_fqn.as_str(), name)
            }
        } else {
            name.to_string()
        }
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

    fn process_function_item(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let ident = node.child_by_field(self.unit, LangRust::field_name);
        let Some(ident) = ident.as_ident() else {
            return None;
        };

        let symbol = self.scopes.find_or_insert_local(node.hir_id(), ident);
        symbol.set_onwer(node.hir_id());
        let fqn = self.scoped_fqn(node, &ident.name);
        dbg!(&fqn);
        // dbg!(&self.scopes.symbols);
        symbol.set_fqn(fqn.clone(), self.unit.interner());

        if let Some(desc) = FunctionDescriptor::from_hir(self.unit, node, fqn.clone()) {
            self.functions.push(desc);
        }

        Some(symbol)
    }

    fn process_let_declaration(&mut self, node: &HirNode<'tcx>) -> Option<SymId> {
        let ident = node.child_by_field(self.unit, LangRust::field_pattern);
        let Some(ident) = ident.as_ident() else {
            return None;
        };

        let symbol = self.scopes.find_or_insert_local(node.hir_id(), ident);
        symbol.set_onwer(node.hir_id());
        let fqn = self.scoped_fqn(node, &ident.name);
        symbol.set_fqn(fqn.clone(), self.unit.interner());

        let var = VariableDescriptor::from_let(self.unit, node, ident.name.clone(), fqn.clone());
        self.variables.push(var);

        Some(symbol.id)
    }

    fn process_parameter(&mut self, node: &HirNode<'tcx>) -> Option<SymId> {
        let ident = node.child_by_field(self.unit, LangRust::field_pattern);
        let Some(ident) = ident.as_ident() else {
            return None;
        };

        let symbol = self.scopes.find_or_insert_local(node.hir_id(), ident);
        symbol.set_onwer(node.hir_id());
        let fqn = self.scoped_fqn(node, &ident.name);
        symbol.set_fqn(fqn.clone(), self.unit.interner());
        Some(symbol.id)
    }

    fn process_const_like(&mut self, node: &HirNode<'tcx>, kind: &'static str) {
        let ident = node.child_by_field(self.unit, LangRust::field_name);
        let Some(ident) = ident.as_ident() else {
            return;
        };

        let symbol = self.scopes.find_or_insert_global(node.hir_id(), ident);
        symbol.set_onwer(node.hir_id());
        let fqn = self.scoped_fqn(node, &ident.name);
        symbol.set_fqn(fqn.clone(), self.unit.interner());

        let variable = match kind {
            "const_item" => VariableDescriptor::from_const_item(
                self.unit,
                node,
                ident.name.clone(),
                fqn.clone(),
            ),
            "static_item" => VariableDescriptor::from_static_item(
                self.unit,
                node,
                ident.name.clone(),
                fqn.clone(),
            ),
            _ => return,
        };

        self.variables.push(variable);
    }

    fn process_struct_item(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let ident = node.child_by_field(self.unit, LangRust::field_name);
        let Some(ident) = ident.as_ident() else {
            return None;
        };

        let symbol = self.scopes.find_or_insert_global(node.hir_id(), ident);
        symbol.set_onwer(node.hir_id());
        let fqn = self.scoped_fqn(node, &ident.name);
        dbg!(&fqn);
        symbol.set_fqn(fqn.clone(), self.unit.interner());

        if let Some(desc) = StructDescriptor::from_struct(self.unit, node, fqn.clone()) {
            self.structs.push(desc);
        }

        Some(symbol)
    }

    fn process_enum_item(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let ident = node.child_by_field(self.unit, LangRust::field_name);
        let Some(ident) = ident.as_ident() else {
            return None;
        };

        let symbol = self.scopes.find_or_insert_global(node.hir_id(), ident);
        symbol.set_onwer(node.hir_id());
        let fqn = self.scoped_fqn(node, &ident.name);
        symbol.set_fqn(fqn.clone(), self.unit.interner());

        if let Some(desc) = EnumDescriptor::from_enum(self.unit, node, fqn.clone()) {
            self.enums.push(desc);
        }

        Some(symbol)
    }

    fn process_mod_item(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let ident = node.child_by_field(self.unit, LangRust::field_name);
        let Some(ident) = ident.as_ident() else {
            return None;
        };

        let symbol = self.scopes.find_or_insert_global(node.hir_id(), ident);
        symbol.set_onwer(node.hir_id());
        let fqn = self.scoped_fqn(node, &ident.name);
        symbol.set_fqn(fqn, self.unit.interner());

        Some(symbol)
    }

    fn process_impl_item(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let type_node = node.opt_child_by_field(self.unit, LangRust::field_type)?;
        let segments = impl_type_segments(self.unit, &type_node)?;
        if segments.is_empty() {
            return None;
        }

        let keys: Vec<_> = segments
            .iter()
            .map(|segment| self.unit.interner().intern(segment))
            .collect();

        if let Some(symbol) = self.scopes.lookup_global_suffix_once(&keys) {
            symbol.set_onwer(node.hir_id());
            return Some(symbol);
        }

        let name = segments.last().cloned().unwrap_or_default();
        let fqn = segments.join("::");
        let symbol = self.unit.new_symbol(node.hir_id(), name);
        symbol.set_onwer(node.hir_id());
        symbol.set_fqn(fqn, self.unit.interner());
        Some(symbol)
    }

    fn visit_children_new_scope(
        &mut self,
        node: &HirNode<'tcx>,
        scoped_symbol: Option<&'tcx Symbol>,
    ) {
        let depth = self.scopes.depth();
        let scope = self.unit.alloc_scope(node.hir_id());

        let symbol = scoped_symbol.or_else(|| self.scopes.scoped_symbol());
        self.scopes.push_with_symbol(scope, symbol);
        self.visit_children(&node);
        self.scopes.pop_until(depth);
    }
}

impl<'tcx> AstVisitorRust<'tcx> for DeclFinder<'tcx> {
    fn unit(&self) -> CompileUnit<'tcx> {
        self.unit
    }

    fn visit_source_file(&mut self, node: HirNode<'tcx>) {
        self.visit_children_new_scope(&node, None);
    }

    fn visit_function_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.process_function_item(&node);
        if let Some(symbol) = symbol {
            self.visit_children_new_scope(&node, Some(symbol));
        } else {
            self.visit_children(&node);
        }
    }

    fn visit_let_declaration(&mut self, node: HirNode<'tcx>) {
        self.process_let_declaration(&node);
        self.visit_children(&node);
    }

    fn visit_block(&mut self, node: HirNode<'tcx>) {
        self.visit_children_new_scope(&node, None);
    }

    fn visit_parameter(&mut self, node: HirNode<'tcx>) {
        self.process_parameter(&node);
        self.visit_children(&node);
    }

    fn visit_mod_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.process_mod_item(&node);
        self.visit_children_new_scope(&node, symbol);
    }

    fn visit_impl_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.process_impl_item(&node);
        self.visit_children_new_scope(&node, symbol);
    }

    fn visit_trait_item(&mut self, node: HirNode<'tcx>) {
        self.visit_children_new_scope(&node, None);
    }

    fn visit_call_expression(&mut self, node: HirNode<'tcx>) {
        let enclosing = self
            .parent_symbol()
            .map(|symbol| symbol.fqn_name.borrow().clone());
        let desc = CallDescriptor::from_call(self.unit, &node, enclosing);
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
        let symbol = self.process_struct_item(&node);
        self.visit_children_new_scope(&node, symbol);
    }

    fn visit_enum_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.process_enum_item(&node);
        self.visit_children_new_scope(&node, symbol);
    }

    fn visit_unknown(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }
}

fn impl_type_segments<'tcx>(
    unit: CompileUnit<'tcx>,
    type_node: &HirNode<'tcx>,
) -> Option<Vec<String>> {
    let ts_node = type_node.inner_ts_node();
    let expr = parse_type_expr(unit, ts_node);
    extract_path_segments(&expr)
}

fn extract_path_segments(expr: &TypeExpr) -> Option<Vec<String>> {
    match expr {
        TypeExpr::Path { segments, .. } => Some(segments.clone()),
        TypeExpr::Reference { inner, .. } => extract_path_segments(inner),
        TypeExpr::Tuple(items) if items.len() == 1 => extract_path_segments(&items[0]),
        _ => None,
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
    unit: CompileUnit<'tcx>,
    globals: &'tcx Scope<'tcx>,
) -> CollectionResult {
    let root = unit.file_start_hir_id().unwrap();
    let node = unit.hir_node(root);
    let mut decl_finder = DeclFinder::new(unit, globals);
    decl_finder.visit_node(node);
    CollectionResult {
        functions: decl_finder.take_functions(),
        variables: decl_finder.take_variables(),
        calls: decl_finder.take_calls(),
        structs: decl_finder.take_structs(),
        enums: decl_finder.take_enums(),
    }
}
