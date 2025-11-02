use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{Scope, Symbol, SymbolKind};

use crate::descriptor::{build_call_descriptor, build_origin};
use crate::token::{AstVisitorPython, LangPython};
use llmcc_descriptor::{
    CallDescriptor, CallKind, ClassDescriptor, ClassField, FunctionDescriptor, FunctionParameter,
    ImportDescriptor, ImportKind, ParameterKind, TypeExpr, VariableDescriptor, VariableScope,
    LANGUAGE_PYTHON,
};

#[derive(Debug)]
pub struct CollectionResult {
    pub functions: Vec<FunctionDescriptor>,
    pub classes: Vec<ClassDescriptor>,
    pub variables: Vec<VariableDescriptor>,
    pub imports: Vec<ImportDescriptor>,
    pub calls: Vec<CallDescriptor>,
}

#[derive(Debug)]
pub struct SymbolSpec {
    pub owner: llmcc_core::ir::HirId,
    pub name: String,
    pub fqn: String,
    pub kind: SymbolKind,
    pub unit_index: usize,
}

#[derive(Debug)]
pub struct ScopeSpec {
    pub owner: Option<llmcc_core::ir::HirId>,
    pub symbol_index: Option<usize>,
    pub symbols: Vec<usize>,
}

#[derive(Debug)]
pub struct CollectedSymbols {
    pub result: CollectionResult,
    pub symbols: Vec<SymbolSpec>,
    pub scopes: Vec<ScopeSpec>,
}

#[derive(Debug)]
pub struct SymbolBatch {
    pub collected: CollectedSymbols,
    pub total_time: Duration,
    pub visit_time: Duration,
}

#[derive(Debug)]
struct ScopeInfo {
    owner: Option<llmcc_core::ir::HirId>,
    symbol_index: Option<usize>,
    symbols: Vec<usize>,
    locals: HashMap<String, usize>,
}

#[derive(Debug)]
struct DeclCollector<'tcx> {
    unit: CompileUnit<'tcx>,
    scope_infos: Vec<ScopeInfo>,
    scope_lookup: HashMap<llmcc_core::ir::HirId, usize>,
    scope_stack: Vec<usize>,
    symbols: Vec<SymbolSpec>,
    functions: Vec<FunctionDescriptor>,
    classes: Vec<ClassDescriptor>,
    variables: Vec<VariableDescriptor>,
    imports: Vec<ImportDescriptor>,
    calls: Vec<CallDescriptor>,
}

impl<'tcx> DeclCollector<'tcx> {
    pub fn new(unit: CompileUnit<'tcx>) -> Self {
        let scope_infos = vec![ScopeInfo {
            owner: None,
            symbol_index: None,
            symbols: Vec::new(),
            locals: HashMap::new(),
        }];

        Self {
            unit,
            scope_infos,
            scope_lookup: HashMap::new(),
            scope_stack: vec![0],
            symbols: Vec::new(),
            functions: Vec::new(),
            classes: Vec::new(),
            variables: Vec::new(),
            imports: Vec::new(),
            calls: Vec::new(),
        }
    }

    fn current_scope_index(&self) -> usize {
        *self
            .scope_stack
            .last()
            .expect("scope stack should never be empty")
    }

    fn ensure_scope(&mut self, owner: llmcc_core::ir::HirId) -> usize {
        if let Some(&idx) = self.scope_lookup.get(&owner) {
            return idx;
        }

        let idx = self.scope_infos.len();
        self.scope_infos.push(ScopeInfo {
            owner: Some(owner),
            symbol_index: None,
            symbols: Vec::new(),
            locals: HashMap::new(),
        });
        self.scope_lookup.insert(owner, idx);
        idx
    }

    fn parent_symbol(&self) -> Option<&SymbolSpec> {
        for &scope_idx in self.scope_stack.iter().rev() {
            if let Some(symbol_idx) = self.scope_infos[scope_idx].symbol_index {
                return self.symbols.get(symbol_idx);
            }
        }
        None
    }

    fn classify_symbol_call(&self, name: &str) -> CallKind {
        if self
            .symbols
            .iter()
            .any(|symbol| symbol.name == name && symbol.kind == SymbolKind::Struct)
        {
            return CallKind::Constructor;
        }

        if is_pascal_case(name) {
            return CallKind::Constructor;
        }

        CallKind::Function
    }

    fn scoped_fqn(&self, _node: &HirNode<'tcx>, name: &str) -> String {
        if let Some(parent) = self.parent_symbol() {
            if parent.fqn.is_empty() {
                name.to_string()
            } else {
                format!("{}::{}", parent.fqn, name)
            }
        } else {
            name.to_string()
        }
    }

    fn create_new_symbol(
        &mut self,
        node: &HirNode<'tcx>,
        field_id: u16,
        global: bool,
        kind: SymbolKind,
    ) -> Option<(usize, String)> {
        let ident_node = node.opt_child_by_field(self.unit, field_id)?;
        let ident = ident_node.as_ident()?;
        let name = ident.name.clone();
        let owner = node.hir_id();

        if let Some(existing_idx) = self.find_symbol_local(&name) {
            let existing_kind = self.symbols[existing_idx].kind;
            if existing_kind != SymbolKind::Unknown && existing_kind != kind {
                let fqn = self.scoped_fqn(node, &name);
                let idx = self.insert_symbol(owner, name.clone(), fqn, kind, global);
                Some((idx, name))
            } else {
                Some((existing_idx, name))
            }
        } else {
            let fqn = self.scoped_fqn(node, &name);
            let idx = self.insert_symbol(owner, name.clone(), fqn, kind, global);
            Some((idx, name))
        }
    }

    fn find_symbol_local(&self, name: &str) -> Option<usize> {
        if self.scope_stack.len() <= 1 {
            return None;
        }

        for &scope_idx in self.scope_stack[1..].iter().rev() {
            if let Some(&symbol_idx) = self.scope_infos[scope_idx].locals.get(name) {
                return Some(symbol_idx);
            }
        }

        None
    }

    fn insert_symbol(
        &mut self,
        owner: llmcc_core::ir::HirId,
        name: String,
        fqn: String,
        kind: SymbolKind,
        global: bool,
    ) -> usize {
        let idx = self.symbols.len();
        self.symbols.push(SymbolSpec {
            owner,
            name: name.clone(),
            fqn,
            kind,
            unit_index: self.unit.index,
        });

        let current_scope = self.current_scope_index();
        self.scope_infos[current_scope]
            .locals
            .insert(name.clone(), idx);
        self.scope_infos[current_scope].symbols.push(idx);

        if global {
            self.scope_infos[0].locals.insert(name.clone(), idx);
            self.scope_infos[0].symbols.push(idx);
        }

        idx
    }

    fn finish(self) -> CollectedSymbols {
        let scope_specs = self
            .scope_infos
            .into_iter()
            .map(|info| ScopeSpec {
                owner: info.owner,
                symbol_index: info.symbol_index,
                symbols: info.symbols,
            })
            .collect();

        CollectedSymbols {
            result: CollectionResult {
                functions: self.functions,
                classes: self.classes,
                variables: self.variables,
                imports: self.imports,
                calls: self.calls,
            },
            symbols: self.symbols,
            scopes: scope_specs,
        }
    }

    fn visit_children_scope(&mut self, node: &HirNode<'tcx>, symbol: Option<usize>) {
        let owner = node.hir_id();
        let scope_idx = self.ensure_scope(owner);
        if let Some(symbol_idx) = symbol {
            self.scope_infos[scope_idx].symbol_index = Some(symbol_idx);
        }

        self.scope_stack.push(scope_idx);
        self.visit_children(node);
        self.scope_stack.pop();
    }

    fn visit_children(&mut self, node: &HirNode<'tcx>) {
        for id in node.children() {
            let child = self.unit.hir_node(*id);
            self.visit_node(child);
        }
    }

    fn module_segments_from_path(path: &Path) -> Vec<String> {
        if path.extension().and_then(|ext| ext.to_str()) != Some("py") {
            return Vec::new();
        }

        let mut segments: Vec<String> = Vec::new();

        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if stem != "__init__" && !stem.is_empty() {
                segments.push(stem.to_string());
            }
        }

        let mut current = path.parent();
        while let Some(dir) = current {
            let dir_name = match dir.file_name().and_then(|n| n.to_str()) {
                Some(name) if !name.is_empty() => name.to_string(),
                _ => break,
            };

            let has_init = dir.join("__init__.py").exists() || dir.join("__init__.pyi").exists();
            if has_init {
                segments.push(dir_name);
                current = dir.parent();
                continue;
            }

            if segments.is_empty() {
                segments.push(dir_name);
            }
            break;
        }

        segments.reverse();
        segments
    }

    fn ensure_module_symbol(&mut self, node: &HirNode<'tcx>) -> Option<usize> {
        let owner = node.hir_id();
        let scope_idx = self.ensure_scope(owner);
        if let Some(symbol_idx) = self.scope_infos[scope_idx].symbol_index {
            return Some(symbol_idx);
        }

        let raw_path = self.unit.file_path().or_else(|| self.unit.file().path());
        let path = raw_path
            .map(PathBuf::from)
            .and_then(|p| p.canonicalize().ok().or(Some(p)))
            .unwrap_or_else(|| PathBuf::from("__module__"));

        let segments = Self::module_segments_from_path(&path);

        let (name, fqn) = if segments.is_empty() {
            let fallback = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("__module__")
                .to_string();
            (fallback.clone(), fallback)
        } else {
            let name = segments
                .last()
                .cloned()
                .unwrap_or_else(|| "__module__".to_string());
            let fqn = segments.join("::");
            (name, fqn)
        };

        let idx = self.symbols.len();
        self.symbols.push(SymbolSpec {
            owner,
            name: name.clone(),
            fqn,
            kind: SymbolKind::Module,
            unit_index: self.unit.index,
        });

        // Module symbols live in the global scope for lookup.
        self.scope_infos[0].locals.insert(name, idx);
        self.scope_infos[0].symbols.push(idx);

        self.scope_infos[scope_idx].symbol_index = Some(idx);
        Some(idx)
    }

    fn extract_base_classes(&mut self, arg_list_node: &HirNode<'tcx>, class: &mut ClassDescriptor) {
        for child_id in arg_list_node.children() {
            let child = self.unit.hir_node(*child_id);
            let text = match child.kind_id() {
                LangPython::identifier => child
                    .as_ident()
                    .map(|ident| ident.name.clone())
                    .unwrap_or_default(),
                _ => self.unit.get_text(
                    child.inner_ts_node().start_byte(),
                    child.inner_ts_node().end_byte(),
                ),
            };

            let trimmed = text.trim();
            if !trimmed.is_empty() {
                class
                    .base_types
                    .push(TypeExpr::opaque(LANGUAGE_PYTHON, trimmed.to_string()));
            }
        }
    }

    fn extract_class_members(&mut self, body_node: &HirNode<'tcx>, class: &mut ClassDescriptor) {
        for child_id in body_node.children() {
            let child = self.unit.hir_node(*child_id);
            let kind_id = child.kind_id();

            if kind_id == LangPython::function_definition {
                if let Some(name_node) = child.opt_child_by_field(self.unit, LangPython::field_name)
                {
                    if let Some(ident) = name_node.as_ident() {
                        class.methods.push(ident.name.clone());
                    }
                }
                self.extract_instance_fields_from_method(&child, class);
            } else if kind_id == LangPython::decorated_definition {
                if let Some(method_name) = self.extract_decorated_method_name(&child) {
                    class.methods.push(method_name);
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

    fn extract_class_field(&self, node: &HirNode<'tcx>) -> Option<ClassField> {
        let left_node = node.opt_child_by_field(self.unit, LangPython::field_left)?;
        let ident = left_node.as_ident()?;

        let mut field = ClassField::new(ident.name.clone());

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
            field.type_annotation = Some(TypeExpr::opaque(LANGUAGE_PYTHON, type_hint));
        }

        Some(field)
    }

    fn upsert_class_field(&self, class: &mut ClassDescriptor, field: ClassField) {
        if let Some(existing) = class.fields.iter_mut().find(|f| f.name == field.name) {
            if existing.type_annotation.is_none() && field.type_annotation.is_some() {
                existing.type_annotation = field.type_annotation;
            }
        } else {
            class.fields.push(field);
        }
    }

    fn extract_instance_fields_from_method(
        &mut self,
        method_node: &HirNode<'tcx>,
        class: &mut ClassDescriptor,
    ) {
        self.collect_instance_fields_recursive(method_node, class);
    }

    fn collect_instance_fields_recursive(
        &mut self,
        node: &HirNode<'tcx>,
        class: &mut ClassDescriptor,
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
        class: &mut ClassDescriptor,
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

        let field = ClassField::new(field_name);
        self.upsert_class_field(class, field);
    }
}

fn is_pascal_case(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !first.is_alphabetic() || !first.is_uppercase() {
        return false;
    }

    let mut has_lowercase = false;
    for ch in chars {
        if ch == '_' {
            return false;
        }
        if ch.is_lowercase() {
            has_lowercase = true;
        }
    }

    has_lowercase
}

impl<'tcx> AstVisitorPython<'tcx> for DeclCollector<'tcx> {
    fn unit(&self) -> CompileUnit<'tcx> {
        self.unit
    }

    fn visit_source_file(&mut self, node: HirNode<'tcx>) {
        let module_symbol = self.ensure_module_symbol(&node);
        self.visit_children_scope(&node, module_symbol);
    }

    fn visit_call(&mut self, node: HirNode<'tcx>) {
        let enclosing = self.parent_symbol().map(|symbol| symbol.fqn.clone());
        let descriptor = build_call_descriptor(self.unit, &node, enclosing, |name| {
            self.classify_symbol_call(name)
        });
        self.calls.push(descriptor);
        self.visit_children(&node);
    }

    fn visit_function_definition(&mut self, node: HirNode<'tcx>) {
        if let Some((symbol_idx, name)) =
            self.create_new_symbol(&node, LangPython::field_name, true, SymbolKind::Function)
        {
            let ts_node = node.inner_ts_node();
            let origin = build_origin(self.unit, &node, ts_node);
            let fqn = self
                .symbols
                .get(symbol_idx)
                .map(|symbol| symbol.fqn.clone());

            let mut func = FunctionDescriptor::new(origin, name.clone());
            func.fqn = fqn;
            func.parameters = collect_function_parameters(self.unit, &node);
            func.return_type = extract_function_return_type(self.unit, &node);
            func.signature = Some(self.unit.get_text(ts_node.start_byte(), ts_node.end_byte()));

            self.functions.push(func);
            self.visit_children_scope(&node, Some(symbol_idx));
        }
    }

    fn visit_class_definition(&mut self, node: HirNode<'tcx>) {
        if let Some((symbol_idx, name)) =
            self.create_new_symbol(&node, LangPython::field_name, true, SymbolKind::Struct)
        {
            let ts_node = node.inner_ts_node();
            let origin = build_origin(self.unit, &node, ts_node);
            let fqn = self
                .symbols
                .get(symbol_idx)
                .map(|symbol| symbol.fqn.clone());

            let mut class = ClassDescriptor::new(origin, name.clone());
            class.fqn = fqn;

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
            self.visit_children_scope(&node, Some(symbol_idx));
        }
    }

    fn visit_decorated_definition(&mut self, node: HirNode<'tcx>) {
        // decorated_definition contains decorators followed by the actual definition (function or class)
        let mut decorators = Vec::new();
        let mut contains_function = false;
        let mut contains_class = false;

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
            } else if kind_id == LangPython::function_definition {
                contains_function = true;
            } else if kind_id == LangPython::class_definition {
                contains_class = true;
            }
        }

        // Visit the decorated definition and apply decorators to the last collected function/class
        self.visit_children(&node);

        // Apply decorators to the last function or class that was added
        if !decorators.is_empty() {
            if contains_function {
                if let Some(last_func) = self.functions.last_mut() {
                    last_func.decorators = decorators.clone();
                }
            } else if contains_class {
                if let Some(last_class) = self.classes.last_mut() {
                    last_class.decorators = decorators.clone();
                }
            }
        }
    }

    fn visit_import_statement(&mut self, node: HirNode<'tcx>) {
        // Handle: import os, sys, etc.
        let mut cursor = node.inner_ts_node().walk();
        let origin = build_origin(self.unit, &node, node.inner_ts_node());

        for child in node.inner_ts_node().children(&mut cursor) {
            if child.kind() == "dotted_name" || child.kind() == "identifier" {
                let text = self.unit.get_text(child.start_byte(), child.end_byte());
                let mut descriptor = ImportDescriptor::new(origin.clone(), text.trim().to_string());
                descriptor.kind = ImportKind::Module;
                self.imports.push(descriptor);
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
        if let Some((symbol_idx, name)) =
            self.create_new_symbol(&node, LangPython::field_left, false, SymbolKind::Variable)
        {
            let ts_node = node.inner_ts_node();
            let origin = build_origin(self.unit, &node, ts_node);
            let mut var = VariableDescriptor::new(origin, name.clone());

            if let Some(symbol) = self.symbols.get(symbol_idx) {
                var.fqn = Some(symbol.fqn.clone());
            }

            var.scope = match self.parent_symbol().map(|spec| spec.kind) {
                Some(SymbolKind::Function) => VariableScope::Function,
                Some(SymbolKind::Struct) => VariableScope::Class,
                Some(SymbolKind::Module) => VariableScope::Module,
                _ => VariableScope::Unknown,
            };

            self.variables.push(var);
        }
    }

    fn visit_unknown(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }
}

fn collect_function_parameters<'tcx>(
    unit: CompileUnit<'tcx>,
    func_node: &HirNode<'tcx>,
) -> Vec<FunctionParameter> {
    let mut params = Vec::new();

    for child_id in func_node.children() {
        let child = unit.hir_node(*child_id);
        if child.kind_id() == LangPython::parameters {
            for param_id in child.children() {
                let param_node = unit.hir_node(*param_id);
                if let Some(mut param) = parse_function_parameter_node(unit, &param_node) {
                    if matches!(param.name.as_deref(), Some("self")) {
                        param.kind = ParameterKind::Receiver;
                    }
                    params.push(param);
                }
            }
        }
    }

    params
}

fn parse_function_parameter_node<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<FunctionParameter> {
    let kind_id = node.kind_id();

    if kind_id == LangPython::Text_COMMA {
        return None;
    }

    if kind_id == LangPython::identifier {
        if let Some(ident) = node.as_ident() {
            let mut param = FunctionParameter::new(Some(ident.name.clone()));
            param.pattern = Some(ident.name.clone());
            return Some(param);
        }
        return None;
    }

    if kind_id == LangPython::typed_parameter || kind_id == LangPython::typed_default_parameter {
        return parse_typed_parameter(unit, node);
    }

    let text = unit.get_text(
        node.inner_ts_node().start_byte(),
        node.inner_ts_node().end_byte(),
    );
    parse_parameter_from_text(&text)
}

fn parse_typed_parameter<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<FunctionParameter> {
    let mut param_name = None;
    let mut type_hint = None;
    let mut default_value = None;

    for child_id in node.children() {
        let child = unit.hir_node(*child_id);
        let kind_id = child.kind_id();

        if kind_id == LangPython::identifier {
            if let Some(ident) = child.as_ident() {
                if param_name.is_none() {
                    param_name = Some(ident.name.clone());
                }
            }
        } else if kind_id == LangPython::type_node {
            let text = unit.get_text(
                child.inner_ts_node().start_byte(),
                child.inner_ts_node().end_byte(),
            );
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                type_hint = Some(trimmed.to_string());
            }
        } else if kind_id != LangPython::Text_COLON && kind_id != LangPython::Text_EQ {
            let text = unit.get_text(
                child.inner_ts_node().start_byte(),
                child.inner_ts_node().end_byte(),
            );
            let trimmed = text.trim();
            if !trimmed.is_empty() && trimmed != "=" && trimmed != ":" {
                default_value = Some(trimmed.to_string());
            }
        }
    }

    let name = param_name?;
    let mut param = FunctionParameter::new(Some(name.clone()));
    param.pattern = Some(name);
    if let Some(type_hint) = type_hint {
        param.type_hint = Some(TypeExpr::opaque(LANGUAGE_PYTHON, type_hint));
    }
    if let Some(default) = default_value {
        param.default_value = Some(default);
    }
    Some(param)
}

fn parse_parameter_from_text(param_text: &str) -> Option<FunctionParameter> {
    let trimmed = param_text.trim();
    if trimmed.is_empty() || matches!(trimmed, "(" | ")") {
        return None;
    }

    let (kind, base) = if let Some(rest) = trimmed.strip_prefix("**") {
        (ParameterKind::VariadicKeyword, rest)
    } else if let Some(rest) = trimmed.strip_prefix('*') {
        (ParameterKind::VariadicPositional, rest)
    } else {
        (ParameterKind::Positional, trimmed)
    };

    let mut name_part = base.trim();
    let mut type_hint = None;
    let mut default_value = None;

    if let Some(colon_pos) = name_part.find(':') {
        let (name, type_part) = name_part.split_at(colon_pos);
        name_part = name;
        let remaining = type_part.trim_start_matches(':').trim();
        if let Some(eq_pos) = remaining.find('=') {
            let (type_text, default_part) = remaining.split_at(eq_pos);
            if !type_text.trim().is_empty() {
                type_hint = Some(type_text.trim().to_string());
            }
            default_value = Some(default_part.trim_start_matches('=').trim().to_string());
        } else if !remaining.is_empty() {
            type_hint = Some(remaining.to_string());
        }
    }

    if let Some(eq_pos) = name_part.find('=') {
        let (name, default_part) = name_part.split_at(eq_pos);
        name_part = name;
        if default_value.is_none() {
            default_value = Some(default_part.trim_start_matches('=').trim().to_string());
        }
    }

    let cleaned_name = name_part.trim();
    let name_option = if cleaned_name.is_empty() {
        None
    } else {
        Some(cleaned_name.to_string())
    };

    let mut param = FunctionParameter::new(name_option.clone());
    param.pattern = Some(trimmed.to_string());
    param.kind = kind;
    if let Some(type_hint) = type_hint {
        param.type_hint = Some(TypeExpr::opaque(LANGUAGE_PYTHON, type_hint));
    }
    if let Some(default) = default_value {
        if !default.is_empty() {
            param.default_value = Some(default);
        }
    }

    Some(param)
}

fn extract_function_return_type<'tcx>(
    unit: CompileUnit<'tcx>,
    func_node: &HirNode<'tcx>,
) -> Option<TypeExpr> {
    let ts_node = func_node.inner_ts_node();
    let mut cursor = ts_node.walk();
    let mut found_arrow = false;

    for child in ts_node.children(&mut cursor) {
        let kind = child.kind();
        if found_arrow {
            if matches!(kind, "type" | "identifier" | "dotted_name") {
                let text = unit.get_text(child.start_byte(), child.end_byte());
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return Some(TypeExpr::opaque(LANGUAGE_PYTHON, trimmed.to_string()));
                }
            } else if child.is_named() {
                let text = unit.get_text(child.start_byte(), child.end_byte());
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return Some(TypeExpr::opaque(LANGUAGE_PYTHON, trimmed.to_string()));
                }
            }
        }

        if kind == "->" {
            found_arrow = true;
        }
    }

    None
}

fn apply_collected_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    globals: &'tcx Scope<'tcx>,
    collected: &CollectedSymbols,
) {
    let interner = unit.interner();
    let mut created_symbols = Vec::with_capacity(collected.symbols.len());

    {
        let mut symbol_map = unit.cc.symbol_map.write();
        for spec in &collected.symbols {
            let key = interner.intern(&spec.name);
            let symbol = unit
                .cc
                .arena
                .alloc(Symbol::new(spec.owner, spec.name.clone(), key));
            symbol.set_kind(spec.kind);
            symbol.set_unit_index(spec.unit_index);
            symbol.set_fqn(spec.fqn.clone(), interner);
            symbol_map.insert(symbol.id, symbol);
            created_symbols.push(symbol);
        }
    }

    for scope in &collected.scopes {
        let target_scope = if let Some(owner) = scope.owner {
            let scope_ref = unit.alloc_scope(owner);
            if let Some(symbol_idx) = scope.symbol_index {
                if let Some(symbol) = created_symbols.get(symbol_idx) {
                    scope_ref.set_symbol(Some(symbol));
                }
            }
            scope_ref
        } else {
            globals
        };

        for &symbol_idx in &scope.symbols {
            if let Some(symbol) = created_symbols.get(symbol_idx) {
                target_scope.insert(symbol, interner);
            }
        }
    }

    // created_symbols intentionally kept for scope insertion above
}

pub fn collect_symbols_batch<'tcx>(unit: CompileUnit<'tcx>) -> SymbolBatch {
    let collect_start = Instant::now();
    let root = unit.file_start_hir_id().unwrap();
    let node = unit.hir_node(root);
    let mut collector = DeclCollector::new(unit);

    let visit_start = Instant::now();
    collector.visit_node(node);
    let visit_time = visit_start.elapsed();

    let collected = collector.finish();

    let total_time = collect_start.elapsed();

    SymbolBatch {
        collected,
        total_time,
        visit_time,
    }
}

pub fn apply_symbol_batch<'tcx>(
    unit: CompileUnit<'tcx>,
    globals: &'tcx Scope<'tcx>,
    batch: SymbolBatch,
) -> CollectionResult {
    let SymbolBatch {
        collected,
        total_time,
        visit_time,
    } = batch;

    let counts = (
        collected.result.functions.len(),
        collected.result.classes.len(),
        collected.result.variables.len(),
        collected.result.imports.len(),
        collected.result.calls.len(),
    );

    apply_collected_symbols(unit, globals, &collected);

    if total_time.as_millis() > 10 {
        tracing::trace!(
            "[COLLECT] File {:?}: total={:.2}ms, visit={:.2}ms, syms={}, classes={}, vars={}, imports={}, calls={}",
            unit.file_path().unwrap_or("unknown"),
            total_time.as_secs_f64() * 1000.0,
            visit_time.as_secs_f64() * 1000.0,
            counts.0,
            counts.1,
            counts.2,
            counts.3,
            counts.4
        );
    }

    let CollectedSymbols { result, .. } = collected;
    result
}

pub fn collect_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    globals: &'tcx Scope<'tcx>,
) -> CollectionResult {
    let batch = collect_symbols_batch(unit);
    apply_symbol_batch(unit, globals, batch)
}
