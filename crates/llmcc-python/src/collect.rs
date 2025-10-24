use std::mem;

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirIdent, HirNode};
use llmcc_core::symbol::{Scope, ScopeStack, Symbol, SymbolKind};

use crate::descriptor::class::PythonClassDescriptor;
use crate::descriptor::function::PythonFunctionDescriptor;
use crate::descriptor::import::ImportDescriptor;
use crate::descriptor::variable::VariableDescriptor;
use crate::token::AstVisitorPython;
use crate::token::LangPython;

#[derive(Debug)]
pub struct CollectionResult {
    pub functions: Vec<PythonFunctionDescriptor>,
    pub classes: Vec<PythonClassDescriptor>,
    pub variables: Vec<VariableDescriptor>,
    pub imports: Vec<ImportDescriptor>,
}

#[derive(Debug)]
struct DeclCollector<'tcx> {
    unit: CompileUnit<'tcx>,
    scopes: ScopeStack<'tcx>,
    functions: Vec<PythonFunctionDescriptor>,
    classes: Vec<PythonClassDescriptor>,
    variables: Vec<VariableDescriptor>,
    imports: Vec<ImportDescriptor>,
}

impl<'tcx> DeclCollector<'tcx> {
    pub fn new(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>) -> Self {
        let mut scopes = ScopeStack::new(&unit.cc.arena, &unit.cc.interner, &unit.cc.symbol_map);
        scopes.push_with_symbol(globals, None);
        Self {
            unit,
            scopes,
            functions: Vec::new(),
            classes: Vec::new(),
            variables: Vec::new(),
            imports: Vec::new(),
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

    fn take_functions(&mut self) -> Vec<PythonFunctionDescriptor> {
        mem::take(&mut self.functions)
    }

    fn take_classes(&mut self) -> Vec<PythonClassDescriptor> {
        mem::take(&mut self.classes)
    }

    fn take_variables(&mut self) -> Vec<VariableDescriptor> {
        mem::take(&mut self.variables)
    }

    fn take_imports(&mut self) -> Vec<ImportDescriptor> {
        mem::take(&mut self.imports)
    }

    fn create_new_symbol(
        &mut self,
        node: &HirNode<'tcx>,
        field_id: u16,
        global: bool,
        kind: SymbolKind,
    ) -> Option<(&'tcx Symbol, &'tcx HirIdent<'tcx>, String)> {
        let ident_node = node.opt_child_by_field(self.unit, field_id)?;
        let ident = ident_node.as_ident()?;
        let fqn = self.scoped_fqn(node, &ident.name);
        let owner = node.hir_id();

        let symbol = match self.scopes.find_symbol_local(&ident.name) {
            Some(existing) if existing.kind() != SymbolKind::Unknown && existing.kind() != kind => {
                self.insert_into_scope(owner, ident, global, &fqn, kind)
            }
            Some(existing) => existing,
            None => self.insert_into_scope(owner, ident, global, &fqn, kind),
        };

        Some((symbol, ident, fqn))
    }

    fn insert_into_scope(
        &mut self,
        owner: llmcc_core::ir::HirId,
        ident: &'tcx HirIdent<'tcx>,
        global: bool,
        fqn: &str,
        kind: SymbolKind,
    ) -> &'tcx Symbol {
        let interner = self.unit.interner();
        let unit_index = self.unit.index;

        self.scopes.insert_with(owner, ident, global, |symbol| {
            symbol.set_owner(owner);
            symbol.set_fqn(fqn.to_string(), interner);
            symbol.set_kind(kind);
            symbol.set_unit_index(unit_index);
        })
    }

    fn visit_children_scope(&mut self, node: &HirNode<'tcx>, symbol: Option<&'tcx Symbol>) {
        let depth = self.scopes.depth();
        // Allocate scope for this node
        let scope = self.unit.alloc_scope(node.hir_id());
        self.scopes.push_with_symbol(scope, symbol);
        self.visit_children(node);
        self.scopes.pop_until(depth);
    }

    fn visit_children(&mut self, node: &HirNode<'tcx>) {
        for id in node.children() {
            let child = self.unit.hir_node(*id);
            self.visit_node(child);
        }
    }

    fn extract_base_classes(
        &mut self,
        arg_list_node: &HirNode<'tcx>,
        class: &mut PythonClassDescriptor,
    ) {
        for child_id in arg_list_node.children() {
            let child = self.unit.hir_node(*child_id);
            if child.kind_id() == LangPython::identifier {
                if let Some(ident) = child.as_ident() {
                    class.add_base_class(ident.name.clone());
                }
            }
        }
    }

    fn extract_class_members(
        &mut self,
        body_node: &HirNode<'tcx>,
        class: &mut PythonClassDescriptor,
    ) {
        for child_id in body_node.children() {
            let child = self.unit.hir_node(*child_id);
            let kind_id = child.kind_id();

            if kind_id == LangPython::function_definition {
                if let Some(name_node) = child.opt_child_by_field(self.unit, LangPython::field_name)
                {
                    if let Some(ident) = name_node.as_ident() {
                        class.add_method(ident.name.clone());
                    }
                }
                self.extract_instance_fields_from_method(&child, class);
            } else if kind_id == LangPython::decorated_definition {
                if let Some(method_name) = self.extract_decorated_method_name(&child) {
                    class.add_method(method_name);
                }
                if let Some(method_node) = self.method_node_from_decorated(&child) {
                    self.extract_instance_fields_from_method(&method_node, class);
                }
            } else if kind_id == LangPython::assignment {
                if let Some(field) = self.extract_class_field(&child) {
                    self.upsert_class_field(class, field);
                }
            } else if kind_id == LangPython::expression_statement {
                for stmt_child_id in child.children() {
                    let stmt_child = self.unit.hir_node(*stmt_child_id);
                    if stmt_child.kind_id() == LangPython::assignment {
                        if let Some(field) = self.extract_class_field(&stmt_child) {
                            self.upsert_class_field(class, field);
                        }
                    }
                }
            }
        }
    }

    fn extract_decorated_method_name(&self, node: &HirNode<'tcx>) -> Option<String> {
        for child_id in node.children() {
            let child = self.unit.hir_node(*child_id);
            if child.kind_id() == LangPython::function_definition {
                if let Some(name_node) = child.opt_child_by_field(self.unit, LangPython::field_name)
                {
                    if let Some(ident) = name_node.as_ident() {
                        return Some(ident.name.clone());
                    }
                }
            }
        }
        None
    }

    fn method_node_from_decorated(&self, node: &HirNode<'tcx>) -> Option<HirNode<'tcx>> {
        for child_id in node.children() {
            let child = self.unit.hir_node(*child_id);
            if child.kind_id() == LangPython::function_definition {
                return Some(child);
            }
        }
        None
    }

    fn extract_class_field(
        &self,
        node: &HirNode<'tcx>,
    ) -> Option<crate::descriptor::class::ClassField> {
        let left_node = node.opt_child_by_field(self.unit, LangPython::field_left)?;
        let ident = left_node.as_ident()?;

        let mut field = crate::descriptor::class::ClassField::new(ident.name.clone());

        let type_hint = node
            .opt_child_by_field(self.unit, LangPython::field_type)
            .and_then(|type_node| {
                let text = self.unit.get_text(
                    type_node.inner_ts_node().start_byte(),
                    type_node.inner_ts_node().end_byte(),
                );
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .or_else(|| {
                for child_id in node.children() {
                    let child = self.unit.hir_node(*child_id);
                    if child.kind_id() == LangPython::type_node {
                        let text = self.unit.get_text(
                            child.inner_ts_node().start_byte(),
                            child.inner_ts_node().end_byte(),
                        );
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            return Some(trimmed.to_string());
                        }
                    }
                }
                None
            });

        if let Some(type_hint) = type_hint {
            field = field.with_type_hint(type_hint);
        }

        Some(field)
    }

    fn upsert_class_field(
        &self,
        class: &mut PythonClassDescriptor,
        field: crate::descriptor::class::ClassField,
    ) {
        if let Some(existing) = class.fields.iter_mut().find(|f| f.name == field.name) {
            if existing.type_hint.is_none() && field.type_hint.is_some() {
                existing.type_hint = field.type_hint;
            }
        } else {
            class.add_field(field);
        }
    }

    fn extract_instance_fields_from_method(
        &mut self,
        method_node: &HirNode<'tcx>,
        class: &mut PythonClassDescriptor,
    ) {
        self.collect_instance_fields_recursive(method_node, class);
    }

    fn collect_instance_fields_recursive(
        &mut self,
        node: &HirNode<'tcx>,
        class: &mut PythonClassDescriptor,
    ) {
        if node.kind_id() == LangPython::assignment {
            self.extract_instance_field_from_assignment(node, class);
        }

        for child_id in node.children() {
            let child = self.unit.hir_node(*child_id);
            self.collect_instance_fields_recursive(&child, class);
        }
    }

    fn extract_instance_field_from_assignment(
        &mut self,
        node: &HirNode<'tcx>,
        class: &mut PythonClassDescriptor,
    ) {
        let left_node = match node.opt_child_by_field(self.unit, LangPython::field_left) {
            Some(node) => node,
            None => return,
        };

        if left_node.kind_id() != LangPython::attribute {
            return;
        }

        let mut identifier_names = Vec::new();
        for child_id in left_node.children() {
            let child = self.unit.hir_node(*child_id);
            if child.kind_id() == LangPython::identifier {
                if let Some(ident) = child.as_ident() {
                    identifier_names.push(ident.name.clone());
                }
            }
        }

        if identifier_names.first().map(String::as_str) != Some("self") {
            return;
        }

        let field_name = match identifier_names.last() {
            Some(name) if name != "self" => name.clone(),
            _ => return,
        };

        let field = crate::descriptor::class::ClassField::new(field_name);
        self.upsert_class_field(class, field);
    }
}

impl<'tcx> AstVisitorPython<'tcx> for DeclCollector<'tcx> {
    fn unit(&self) -> CompileUnit<'tcx> {
        self.unit
    }

    fn visit_source_file(&mut self, node: HirNode<'tcx>) {
        self.visit_children_scope(&node, None);
    }

    fn visit_function_definition(&mut self, node: HirNode<'tcx>) {
        if let Some((symbol, ident, _fqn)) =
            self.create_new_symbol(&node, LangPython::field_name, true, SymbolKind::Function)
        {
            let mut func = PythonFunctionDescriptor::new(ident.name.clone());

            // Extract parameters and return type using AST walking methods
            for child_id in node.children() {
                let child = self.unit.hir_node(*child_id);
                let kind_id = child.kind_id();

                if kind_id == LangPython::parameters {
                    func.extract_parameters_from_ast(&child, self.unit);
                }
            }

            // Extract return type by walking the AST
            func.extract_return_type_from_ast(&node, self.unit);

            self.functions.push(func);
            self.visit_children_scope(&node, Some(symbol));
        }
    }

    fn visit_class_definition(&mut self, node: HirNode<'tcx>) {
        if let Some((symbol, ident, _fqn)) =
            self.create_new_symbol(&node, LangPython::field_name, true, SymbolKind::Struct)
        {
            let mut class = PythonClassDescriptor::new(ident.name.clone());

            // Look for base classes and body
            for child_id in node.children() {
                let child = self.unit.hir_node(*child_id);
                let kind_id = child.kind_id();

                if kind_id == LangPython::argument_list {
                    // These are base classes
                    self.extract_base_classes(&child, &mut class);
                } else if kind_id == LangPython::block {
                    // This is the class body
                    self.extract_class_members(&child, &mut class);
                }
            }

            self.classes.push(class);
            self.visit_children_scope(&node, Some(symbol));
        }
    }

    fn visit_decorated_definition(&mut self, node: HirNode<'tcx>) {
        // decorated_definition contains decorators followed by the actual definition (function or class)
        let mut decorators = Vec::new();

        for child_id in node.children() {
            let child = self.unit.hir_node(*child_id);
            let kind_id = child.kind_id();

            if kind_id == LangPython::decorator {
                // Extract decorator name
                // A decorator is usually just an identifier or a call expression
                // For now, extract the text of the decorator
                let decorator_text = self.unit.get_text(
                    child.inner_ts_node().start_byte(),
                    child.inner_ts_node().end_byte(),
                );
                if !decorator_text.is_empty() {
                    decorators.push(decorator_text.trim_start_matches('@').trim().to_string());
                }
            }
        }

        // Visit the decorated definition and apply decorators to the last collected function/class
        self.visit_children(&node);

        // Apply decorators to the last function or class that was added
        if !decorators.is_empty() {
            if let Some(last_func) = self.functions.last_mut() {
                last_func.decorators = decorators.clone();
            }
        }
    }

    fn visit_import_statement(&mut self, node: HirNode<'tcx>) {
        // Handle: import os, sys, etc.
        let mut cursor = node.inner_ts_node().walk();

        for child in node.inner_ts_node().children(&mut cursor) {
            if child.kind() == "dotted_name" || child.kind() == "identifier" {
                let text = self.unit.get_text(child.start_byte(), child.end_byte());
                let _import =
                    ImportDescriptor::new(text, crate::descriptor::import::ImportKind::Simple);
                self.imports.push(_import);
            }
        }
    }

    fn visit_import_from(&mut self, _node: HirNode<'tcx>) {
        // Handle: from x import y
        // This is more complex - we need to parse module and names
        // For now, simple implementation
    }

    fn visit_assignment(&mut self, node: HirNode<'tcx>) {
        // Handle: x = value
        // In tree-sitter, the "left" side of assignment is the target
        if let Some((_symbol, ident, _)) =
            self.create_new_symbol(&node, LangPython::field_left, false, SymbolKind::Variable)
        {
            use crate::descriptor::variable::VariableScope;
            let var = VariableDescriptor::new(ident.name.clone(), VariableScope::FunctionLocal);
            self.variables.push(var);
        }
    }

    fn visit_unknown(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }
}

pub fn collect_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    globals: &'tcx Scope<'tcx>,
) -> CollectionResult {
    let root = unit.file_start_hir_id().unwrap();
    let node = unit.hir_node(root);
    let mut collector = DeclCollector::new(unit, globals);
    collector.visit_node(node);

    CollectionResult {
        functions: collector.take_functions(),
        classes: collector.take_classes(),
        variables: collector.take_variables(),
        imports: collector.take_imports(),
    }
}
