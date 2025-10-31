use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{Scope, Symbol, SymbolKind};

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
    functions: Vec<PythonFunctionDescriptor>,
    classes: Vec<PythonClassDescriptor>,
    variables: Vec<VariableDescriptor>,
    imports: Vec<ImportDescriptor>,
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
        let module_symbol = self.ensure_module_symbol(&node);
        self.visit_children_scope(&node, module_symbol);
    }

    fn visit_function_definition(&mut self, node: HirNode<'tcx>) {
        if let Some((symbol_idx, name)) =
            self.create_new_symbol(&node, LangPython::field_name, true, SymbolKind::Function)
        {
            let mut func = PythonFunctionDescriptor::new(name.clone());

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
            self.visit_children_scope(&node, Some(symbol_idx));
        }
    }

    fn visit_class_definition(&mut self, node: HirNode<'tcx>) {
        if let Some((symbol_idx, name)) =
            self.create_new_symbol(&node, LangPython::field_name, true, SymbolKind::Struct)
        {
            let mut class = PythonClassDescriptor::new(name.clone());

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
        if let Some((_symbol_idx, name)) =
            self.create_new_symbol(&node, LangPython::field_left, false, SymbolKind::Variable)
        {
            use crate::descriptor::variable::VariableScope;
            let var = VariableDescriptor::new(name, VariableScope::FunctionLocal);
            self.variables.push(var);
        }
    }

    fn visit_unknown(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }
}

fn apply_collected_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    globals: &'tcx Scope<'tcx>,
    collected: &CollectedSymbols,
) {
    let interner = unit.interner();
    let mut created_symbols = Vec::with_capacity(collected.symbols.len());

    {
        let mut symbol_map = unit.cc.symbol_map.write().unwrap();
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
    );

    apply_collected_symbols(unit, globals, &collected);

    if total_time.as_millis() > 10 {
        tracing::trace!(
            "[COLLECT] File {:?}: total={:.2}ms, visit={:.2}ms, syms={}, classes={}, vars={}, imports={}",
            unit.file_path().unwrap_or("unknown"),
            total_time.as_secs_f64() * 1000.0,
            visit_time.as_secs_f64() * 1000.0,
            counts.0,
            counts.1,
            counts.2,
            counts.3
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
