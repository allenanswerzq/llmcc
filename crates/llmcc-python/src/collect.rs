use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirId, HirNode};
use llmcc_core::symbol::{Scope, Symbol, SymbolKind};

use crate::describe::PythonDescriptorBuilder;
use crate::token::{AstVisitorPython, LangPython};
use llmcc_descriptor::{
    CallDescriptor, CallKind, CallTarget, ClassDescriptor, DescriptorTrait, FunctionDescriptor,
    ImportDescriptor, TypeExpr, VariableDescriptor, VariableScope, LANGUAGE_PYTHON,
};

#[derive(Debug)]
pub struct CollectionResult {
    pub functions: Vec<FunctionDescriptor>,
    pub function_map: HashMap<HirId, usize>,
    pub classes: Vec<ClassDescriptor>,
    pub class_map: HashMap<HirId, usize>,
    pub variables: Vec<VariableDescriptor>,
    pub variable_map: HashMap<HirId, usize>,
    pub imports: Vec<ImportDescriptor>,
    pub import_map: HashMap<HirId, usize>,
    pub calls: Vec<CallDescriptor>,
    pub call_map: HashMap<HirId, usize>,
}

#[derive(Debug)]
pub struct SymbolSpec {
    pub owner: llmcc_core::ir::HirId,
    pub name: String,
    pub fqn: String,
    pub kind: SymbolKind,
    pub unit_index: usize,
    pub is_global: bool,
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
    function_map: HashMap<HirId, usize>,
    classes: Vec<ClassDescriptor>,
    class_map: HashMap<HirId, usize>,
    variables: Vec<VariableDescriptor>,
    variable_map: HashMap<HirId, usize>,
    imports: Vec<ImportDescriptor>,
    import_map: HashMap<HirId, usize>,
    calls: Vec<CallDescriptor>,
    call_map: HashMap<HirId, usize>,
}

#[allow(clippy::needless_lifetimes)]
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
            function_map: HashMap::new(),
            classes: Vec::new(),
            class_map: HashMap::new(),
            variables: Vec::new(),
            variable_map: HashMap::new(),
            imports: Vec::new(),
            import_map: HashMap::new(),
            calls: Vec::new(),
            call_map: HashMap::new(),
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
                if global {
                    self.symbols[existing_idx].is_global = true;
                }
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
            is_global: global,
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
                function_map: self.function_map,
                classes: self.classes,
                class_map: self.class_map,
                variables: self.variables,
                variable_map: self.variable_map,
                imports: self.imports,
                import_map: self.import_map,
                calls: self.calls,
                call_map: self.call_map,
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
            is_global: true,
        });

        // Module symbols live in the global scope for lookup.
        self.scope_infos[0].locals.insert(name, idx);
        self.scope_infos[0].symbols.push(idx);

        self.scope_infos[scope_idx].symbol_index = Some(idx);
        Some(idx)
    }

    fn extract_assignment_type(&self, node: &HirNode<'tcx>) -> Option<String> {
        if let Some(type_node) = node.opt_child_by_field(self.unit, LangPython::field_type) {
            let text = self.unit.get_text(
                type_node.inner_ts_node().start_byte(),
                type_node.inner_ts_node().end_byte(),
            );
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }

        let assignment_text = self.unit.get_text(
            node.inner_ts_node().start_byte(),
            node.inner_ts_node().end_byte(),
        );
        let colon_index = assignment_text.find(':')?;
        let after_colon = assignment_text[colon_index + 1..].trim();
        if after_colon.is_empty() {
            return None;
        }

        let annotation = after_colon.split('=').next().map(str::trim).unwrap_or("");

        if annotation.is_empty() {
            None
        } else {
            Some(annotation.to_string())
        }
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
        if let Some(mut descriptor) = PythonDescriptorBuilder::build_call(self.unit, &node) {
            self.apply_call_kind_hint(&mut descriptor);
            let idx = self.calls.len();
            self.calls.push(descriptor);
            self.call_map.insert(node.hir_id(), idx);
        }
        self.visit_children(&node);
    }

    fn visit_function_definition(&mut self, node: HirNode<'tcx>) {
        if let Some((symbol_idx, _name)) =
            self.create_new_symbol(&node, LangPython::field_name, true, SymbolKind::Function)
        {
            let fqn = self.symbols[symbol_idx].fqn.clone();
            if let Some(mut func) = PythonDescriptorBuilder::build_function(self.unit, &node) {
                func.fqn = Some(fqn);
                let idx = self.functions.len();
                self.functions.push(func);
                self.function_map.insert(node.hir_id(), idx);
            }
            self.visit_children_scope(&node, Some(symbol_idx));
        }
    }

    fn visit_class_definition(&mut self, node: HirNode<'tcx>) {
        if let Some((symbol_idx, _name)) =
            self.create_new_symbol(&node, LangPython::field_name, true, SymbolKind::Struct)
        {
            let fqn = self.symbols[symbol_idx].fqn.clone();
            if let Some(mut class) = PythonDescriptorBuilder::build_impl(self.unit, &node) {
                class.fqn = Some(fqn);
                let idx = self.classes.len();
                self.classes.push(class);
                self.class_map.insert(node.hir_id(), idx);
            }
            self.visit_children_scope(&node, Some(symbol_idx));
        }
    }

    fn visit_decorated_definition(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }

    fn visit_import_statement(&mut self, node: HirNode<'tcx>) {
        if let Some(descriptor) = PythonDescriptorBuilder::build_import(self.unit, &node) {
            let idx = self.imports.len();
            self.imports.push(descriptor);
            self.import_map.insert(node.hir_id(), idx);
        }
    }

    fn visit_import_from(&mut self, node: HirNode<'tcx>) {
        if let Some(descriptor) = PythonDescriptorBuilder::build_import(self.unit, &node) {
            let idx = self.imports.len();
            self.imports.push(descriptor);
            self.import_map.insert(node.hir_id(), idx);
        }
    }

    fn visit_assignment(&mut self, node: HirNode<'tcx>) {
        // Handle: x = value
        // In tree-sitter, the "left" side of assignment is the target
        if let Some((symbol_idx, name)) =
            self.create_new_symbol(&node, LangPython::field_left, false, SymbolKind::Variable)
        {
            let fqn = self.symbols[symbol_idx].fqn.clone();
            let scope = match self.parent_symbol().map(|spec| spec.kind) {
                Some(SymbolKind::Function) => VariableScope::Function,
                Some(SymbolKind::Struct) => VariableScope::Class,
                Some(SymbolKind::Module) => VariableScope::Module,
                _ => VariableScope::Unknown,
            };

            if let Some(mut var) = PythonDescriptorBuilder::build_variable(self.unit, &node) {
                var.fqn = Some(fqn);
                var.name = name.clone();
                var.scope = scope;
                if var.type_annotation.is_none() {
                    if let Some(annotation) = self.extract_assignment_type(&node) {
                        var.type_annotation = Some(TypeExpr::opaque(LANGUAGE_PYTHON, annotation));
                    }
                }
                let idx = self.variables.len();
                self.variables.push(var);
                self.variable_map.insert(node.hir_id(), idx);
            }
        }

        self.visit_children(&node);
    }

    fn visit_unknown(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }
}
#[allow(clippy::needless_lifetimes)]
impl<'tcx> DeclCollector<'tcx> {
    fn apply_call_kind_hint(&self, descriptor: &mut CallDescriptor) {
        if let CallTarget::Symbol(symbol) = &mut descriptor.target {
            symbol.kind = self.classify_symbol_call(&symbol.name);
        }
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
            symbol.set_is_global(spec.is_global);
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

pub fn collect_symbols_batch(unit: CompileUnit<'_>) -> SymbolBatch {
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
