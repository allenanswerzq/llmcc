use llmcc_core::context::CompileUnit;
use llmcc_core::interner::InternedStr;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{Scope, ScopeStack, Symbol, SymbolKind};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::describe::PythonDescriptorBuilder;
use crate::token::{AstVisitorPython, LangPython};
use llmcc_descriptor::{CallChain, DescriptorMeta, LanguageDescriptorBuilder};

#[derive(Debug, Default)]
pub struct BindingResult {
    pub calls: Vec<CallBinding>,
}

#[derive(Debug, Clone)]
pub struct CallBinding {
    pub caller: String,
    pub target: String,
}

#[derive(Debug)]
struct SymbolBinder<'tcx> {
    unit: CompileUnit<'tcx>,
    scopes: ScopeStack<'tcx>,
    calls: Vec<CallBinding>,
    module_imports: Vec<&'tcx Symbol>,
}

impl<'tcx> SymbolBinder<'tcx> {
    pub fn new(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>) -> Self {
        let mut scopes = ScopeStack::new(&unit.cc.arena, &unit.cc.interner, &unit.cc.symbol_map);
        scopes.push(globals);
        Self {
            unit,
            scopes,
            calls: Vec::new(),
            module_imports: Vec::new(),
        }
    }

    fn interner(&self) -> &llmcc_core::interner::InternPool {
        self.unit.interner()
    }

    fn current_symbol(&self) -> Option<&'tcx Symbol> {
        self.scopes.scoped_symbol()
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

    fn ensure_module_symbol(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let scope = self.unit.alloc_scope(node.hir_id());
        if let Some(symbol) = scope.symbol() {
            return Some(symbol);
        }

        let raw_path = self.unit.file_path().or_else(|| self.unit.file().path());
        let path = raw_path
            .map(PathBuf::from)
            .and_then(|p| p.canonicalize().ok().or(Some(p)))
            .unwrap_or_else(|| PathBuf::from("__module__"));

        let segments = Self::module_segments_from_path(&path);
        let interner = self.unit.interner();

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

        let key = interner.intern(&name);
        let symbol = Symbol::new(node.hir_id(), name.clone(), key);
        let symbol = self.unit.cc.arena.alloc(symbol);
        symbol.set_kind(SymbolKind::Module);
        symbol.set_unit_index(self.unit.index);
        symbol.set_fqn(fqn, interner);

        self.unit.cc.symbol_map.write().insert(symbol.id, symbol);

        let _ = self.scopes.insert_symbol(symbol, true);
        scope.set_symbol(Some(symbol));
        Some(symbol)
    }

    #[allow(dead_code)]
    fn visit_children_scope(&mut self, node: &HirNode<'tcx>, symbol: Option<&'tcx Symbol>) {
        let depth = self.scopes.depth();
        if let Some(symbol) = symbol {
            if let Some(parent) = self.scopes.scoped_symbol() {
                parent.add_dependency(symbol);
            }
        }

        let scope = self.unit.opt_get_scope(node.hir_id());
        if let Some(scope) = scope {
            self.scopes.push_with_symbol(scope, symbol);
            self.visit_children(node);
            self.scopes.pop_until(depth);
        } else {
            self.visit_children(node);
        }
    }

    fn lookup_symbol_suffix(
        &mut self,
        suffix: &[InternedStr],
        kind: Option<SymbolKind>,
    ) -> Option<&'tcx Symbol> {
        let file_index = self.unit.index;
        self.scopes
            .find_scoped_suffix_with_filters(suffix, kind, Some(file_index))
            .or_else(|| {
                self.scopes
                    .find_scoped_suffix_with_filters(suffix, kind, None)
            })
            .or_else(|| {
                self.scopes
                    .find_global_suffix_with_filters(suffix, kind, Some(file_index))
            })
            .or_else(|| {
                self.scopes
                    .find_global_suffix_with_filters(suffix, kind, None)
            })
    }

    fn add_symbol_relation(&mut self, symbol: Option<&'tcx Symbol>) {
        let Some(target) = symbol else { return };
        let Some(current) = self.current_symbol() else {
            return;
        };

        current.add_dependency(target);

        match current.kind() {
            SymbolKind::Function => {
                let parent_class = self
                    .scopes
                    .iter()
                    .rev()
                    .filter_map(|scope| scope.symbol())
                    .find(|symbol| symbol.kind() == SymbolKind::Struct);

                if let Some(class_symbol) = parent_class {
                    class_symbol.add_dependency(target);
                }
            }
            SymbolKind::Module => {
                if !self.module_imports.iter().any(|&sym| sym.id == target.id) {
                    self.module_imports.push(target);
                }
            }
            _ => {}
        }
    }

    fn record_segments_dependency(&mut self, segments: &[String]) {
        if segments.is_empty() {
            return;
        }

        let interner = self.interner();
        let suffix: Vec<_> = segments.iter().rev().map(|s| interner.intern(s)).collect();

        let target = self
            .lookup_symbol_suffix(&suffix, Some(SymbolKind::Struct))
            .or_else(|| self.lookup_symbol_suffix(&suffix, Some(SymbolKind::Enum)))
            .or_else(|| self.lookup_symbol_suffix(&suffix, Some(SymbolKind::Module)))
            .or_else(|| self.lookup_symbol_suffix(&suffix, None));

        self.add_symbol_relation(target);
    }

    fn build_attribute_path(&mut self, node: &HirNode<'tcx>, out: &mut Vec<String>) {
        if node.kind_id() == LangPython::attribute {
            if let Some(object_node) = node.opt_child_by_field(self.unit, LangPython::field_object)
            {
                self.build_attribute_path(&object_node, out);
            }
            if let Some(attr_node) = node.opt_child_by_field(self.unit, LangPython::field_attribute)
            {
                if let Some(ident) = attr_node.as_ident() {
                    out.push(ident.name.clone());
                }
            }
        } else if node.kind_id() == LangPython::identifier {
            if let Some(ident) = node.as_ident() {
                out.push(ident.name.clone());
            }
        } else {
            for child_id in node.children() {
                let child = self.unit.hir_node(*child_id);
                self.build_attribute_path(&child, out);
            }
        }
    }

    fn collect_identifier_paths(&mut self, node: &HirNode<'tcx>, results: &mut Vec<Vec<String>>) {
        if node.kind_id() == LangPython::identifier {
            if let Some(ident) = node.as_ident() {
                results.push(vec![ident.name.clone()]);
            }
            return;
        }

        if node.kind_id() == LangPython::attribute {
            let mut path = Vec::new();
            self.build_attribute_path(node, &mut path);
            if !path.is_empty() {
                results.push(path);
            }
            return;
        }

        for child_id in node.children() {
            let child = self.unit.hir_node(*child_id);
            self.collect_identifier_paths(&child, results);
        }
    }

    fn add_type_dependencies(&mut self, node: &HirNode<'tcx>) {
        let mut paths = Vec::new();
        self.collect_identifier_paths(node, &mut paths);

        let mut seen = HashSet::new();
        for path in paths {
            if path.is_empty() {
                continue;
            }
            let key = path.join("::");
            if seen.insert(key) {
                self.record_segments_dependency(&path);
            }
        }
    }

    fn record_import_path(&mut self, path: &str) {
        let segments: Vec<String> = path
            .split('.')
            .filter(|segment| !segment.is_empty())
            .map(|segment| segment.trim().to_string())
            .collect();
        if segments.is_empty() {
            return;
        }

        let interner = self.interner();
        let suffix: Vec<_> = segments.iter().rev().map(|s| interner.intern(s)).collect();

        let target = self
            .lookup_symbol_suffix(&suffix, Some(SymbolKind::Struct))
            .or_else(|| self.lookup_symbol_suffix(&suffix, Some(SymbolKind::Enum)))
            .or_else(|| self.lookup_symbol_suffix(&suffix, Some(SymbolKind::Module)))
            .or_else(|| self.lookup_symbol_suffix(&suffix, None));

        self.add_symbol_relation(target);
    }

    fn visit_children(&mut self, node: &HirNode<'tcx>) {
        // Use HIR children instead of tree-sitter children
        for child_id in node.children() {
            let child = self.unit.hir_node(*child_id);
            self.visit_node(child);
        }
    }

    fn visit_decorated_def(&mut self, node: &HirNode<'tcx>) {
        let mut decorator_symbols = Vec::new();
        let mut definition_idx = None;

        for (idx, child_id) in node.children().iter().enumerate() {
            let child = self.unit.hir_node(*child_id);
            let kind_id = child.kind_id();

            if kind_id == LangPython::decorator {
                let content = self.unit.file().content();
                let ts_node = child.inner_ts_node();
                if let Ok(decorator_text) = ts_node.utf8_text(content) {
                    let decorator_name = decorator_text.trim_start_matches('@').trim();
                    let key = self.interner().intern(decorator_name);
                    if let Some(decorator_symbol) =
                        self.lookup_symbol_suffix(&[key], Some(SymbolKind::Function))
                    {
                        decorator_symbols.push(decorator_symbol);
                    }
                }
            } else if kind_id == LangPython::function_definition
                || kind_id == LangPython::class_definition
            {
                definition_idx = Some(idx);
                break;
            }
        }

        if let Some(idx) = definition_idx {
            let definition_id = node.children()[idx];
            let definition = self.unit.hir_node(definition_id);
            self.visit_definition_node(&definition, &decorator_symbols);
        }
    }

    fn visit_call_impl(&mut self, node: &HirNode<'tcx>) {
        let enclosing = self
            .current_symbol()
            .map(|symbol| symbol.fqn_name.read().clone());

        if let Some(descriptor) = PythonDescriptorBuilder::build_call_descriptor(
            self.unit,
            node,
            DescriptorMeta::Call {
                enclosing: enclosing.as_deref(),
                fqn: None,
                kind_hint: None,
            },
        ) {
            self.process_call_descriptor(&descriptor);
        }

        self.visit_children(node);
    }

    fn process_call_descriptor(&mut self, descriptor: &llmcc_descriptor::CallDescriptor) {
        match &descriptor.target {
            llmcc_descriptor::CallTarget::Symbol(symbol) => {
                let mut segments = symbol.qualifiers.clone();
                segments.push(symbol.name.clone());
                if !self.handle_symbol_segments(&segments) {
                    self.handle_symbol_segments(&[symbol.name.clone()]);
                }
            }
            llmcc_descriptor::CallTarget::Chain(chain) => {
                if let Some(target) = self.resolve_method_from_chain(chain) {
                    self.add_symbol_relation(Some(target));
                    self.record_call_binding(target);
                } else if let Some(segment) = chain.segments.last() {
                    self.handle_symbol_segments(&[segment.name.clone()]);
                }
            }
            llmcc_descriptor::CallTarget::Dynamic { .. } => {}
        }
    }

    fn handle_symbol_segments(&mut self, segments: &[String]) -> bool {
        if segments.is_empty() {
            return false;
        }

        let keys: Vec<InternedStr> = segments
            .iter()
            .map(|segment| self.interner().intern(segment))
            .collect();

        if let Some(target) = self.lookup_symbol_suffix(&keys, Some(SymbolKind::Function)) {
            self.add_symbol_relation(Some(target));
            self.record_call_binding(target);
            return true;
        }

        if let Some(target) = self.lookup_symbol_suffix(&keys, Some(SymbolKind::Struct)) {
            self.add_symbol_relation(Some(target));
            return true;
        }

        if let Some(target) = self.lookup_symbol_suffix(&keys, None) {
            self.add_symbol_relation(Some(target));
            if target.kind() == SymbolKind::Function {
                self.record_call_binding(target);
            }
            return true;
        }

        false
    }

    fn record_call_binding(&mut self, target: &Symbol) {
        let caller_name = self
            .current_symbol()
            .map(|s| s.fqn_name.read().clone())
            .unwrap_or_else(|| "<module>".to_string());
        let target_name = target.fqn_name.read().clone();
        self.calls.push(CallBinding {
            caller: caller_name,
            target: target_name,
        });
    }

    fn resolve_method_from_chain(&mut self, chain: &CallChain) -> Option<&'tcx Symbol> {
        let segment = chain.segments.last()?;
        if chain.root == "self" {
            if let Some(class_symbol) = self
                .scopes
                .iter()
                .rev()
                .filter_map(|scope| scope.symbol())
                .find(|symbol| symbol.kind() == SymbolKind::Struct)
            {
                let method_fqn = format!("{}::{}", class_symbol.fqn_name.read(), segment.name);
                let key = self.interner().intern(&method_fqn);
                return self
                    .lookup_symbol_suffix(&[key], Some(SymbolKind::Function))
                    .or_else(|| self.lookup_symbol_suffix(&[key], None));
            }
        }

        None
    }

    fn visit_definition_node(&mut self, node: &HirNode<'tcx>, decorator_symbols: &[&'tcx Symbol]) {
        let kind_id = node.kind_id();
        let name_node = match node.opt_child_by_field(self.unit, LangPython::field_name) {
            Some(name) => name,
            None => {
                self.visit_children(node);
                return;
            }
        };

        let ident = match name_node.as_ident() {
            Some(ident) => ident,
            None => {
                self.visit_children(node);
                return;
            }
        };

        let key = self.interner().intern(&ident.name);
        let preferred_kind = if kind_id == LangPython::function_definition {
            Some(SymbolKind::Function)
        } else if kind_id == LangPython::class_definition {
            Some(SymbolKind::Struct)
        } else {
            None
        };

        let mut symbol = preferred_kind
            .and_then(|kind| self.lookup_symbol_suffix(&[key], Some(kind)))
            .or_else(|| self.lookup_symbol_suffix(&[key], None));

        let parent_symbol = self.current_symbol();

        if let Some(scope) = self.unit.opt_get_scope(node.hir_id()) {
            if symbol.is_none() {
                symbol = scope.symbol();
            }

            let depth = self.scopes.depth();
            self.scopes.push_with_symbol(scope, symbol);

            if let Some(current_symbol) = self.current_symbol() {
                if kind_id == LangPython::function_definition {
                    if let Some(class_symbol) = parent_symbol {
                        if class_symbol.kind() == SymbolKind::Struct {
                            class_symbol.add_dependency(current_symbol);
                        }
                    }
                } else if kind_id == LangPython::class_definition {
                    self.add_base_class_dependencies(node, current_symbol);
                }

                for decorator_symbol in decorator_symbols {
                    current_symbol.add_dependency(decorator_symbol);
                }
            }

            self.visit_children(node);
            self.scopes.pop_until(depth);
        } else {
            self.visit_children(node);
        }
    }

    fn add_base_class_dependencies(&mut self, node: &HirNode<'tcx>, class_symbol: &Symbol) {
        for child_id in node.children() {
            let child = self.unit.hir_node(*child_id);
            if child.kind_id() == LangPython::argument_list {
                for base_id in child.children() {
                    let base_node = self.unit.hir_node(*base_id);

                    if let Some(ident) = base_node.as_ident() {
                        let key = self.interner().intern(&ident.name);
                        if let Some(base_symbol) =
                            self.lookup_symbol_suffix(&[key], Some(SymbolKind::Struct))
                        {
                            class_symbol.add_dependency(base_symbol);
                        }
                    } else if base_node.kind_id() == LangPython::attribute {
                        if let Some(attr_node) =
                            base_node.inner_ts_node().child_by_field_name("attribute")
                        {
                            let content = self.unit.file().content();
                            if let Ok(name) = attr_node.utf8_text(content) {
                                let key = self.interner().intern(name);
                                if let Some(base_symbol) =
                                    self.lookup_symbol_suffix(&[key], Some(SymbolKind::Struct))
                                {
                                    class_symbol.add_dependency(base_symbol);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn add_parameter_type_dependencies(&mut self, params_node: &HirNode<'tcx>) {
        for child_id in params_node.children() {
            let child = self.unit.hir_node(*child_id);
            match child.kind_id() {
                id if id == LangPython::typed_parameter
                    || id == LangPython::typed_default_parameter =>
                {
                    self.collect_parameter_type_annotations(&child);
                }
                id if id == LangPython::type_node => {
                    self.add_type_dependencies(&child);
                }
                _ => {}
            }
        }
    }

    fn collect_parameter_type_annotations(&mut self, param_node: &HirNode<'tcx>) {
        for child_id in param_node.children() {
            let child = self.unit.hir_node(*child_id);
            if child.kind_id() == LangPython::type_node {
                self.add_type_dependencies(&child);
            }
        }
    }

    fn propagate_child_dependencies(&mut self, parent: &'tcx Symbol, child: &'tcx Symbol) {
        let dependencies: Vec<_> = child.depends.read().clone();
        for dep_id in dependencies {
            if dep_id == parent.id {
                continue;
            }

            if let Some(dep_symbol) = self.unit.opt_get_symbol(dep_id) {
                if dep_symbol.kind() == SymbolKind::Function {
                    continue;
                }

                if dep_symbol.depends.read().contains(&parent.id) {
                    continue;
                }

                parent.add_dependency(dep_symbol);
            }
        }
    }
}

impl<'tcx> AstVisitorPython<'tcx> for SymbolBinder<'tcx> {
    fn unit(&self) -> CompileUnit<'tcx> {
        self.unit
    }

    fn visit_source_file(&mut self, node: HirNode<'tcx>) {
        self.module_imports.clear();
        let module_symbol = self.ensure_module_symbol(&node);
        self.visit_children_scope(&node, module_symbol);
    }

    fn visit_function_definition(&mut self, node: HirNode<'tcx>) {
        let name_node = match node.opt_child_by_field(self.unit, LangPython::field_name) {
            Some(n) => n,
            None => {
                self.visit_children(&node);
                return;
            }
        };

        let ident = match name_node.as_ident() {
            Some(id) => id,
            None => {
                self.visit_children(&node);
                return;
            }
        };

        let key = self.interner().intern(&ident.name);
        let mut symbol = self.lookup_symbol_suffix(&[key], Some(SymbolKind::Function));

        // Get the parent symbol before pushing a new scope
        let parent_symbol = self.current_symbol();

        if let Some(scope) = self.unit.opt_get_scope(node.hir_id()) {
            // If symbol not found by lookup, get it from the scope
            if symbol.is_none() {
                symbol = scope.symbol();
            }

            let depth = self.scopes.depth();
            self.scopes.push_with_symbol(scope, symbol);

            if let Some(current_symbol) = self.current_symbol() {
                // If parent is a class, class depends on method
                if let Some(parent) = parent_symbol {
                    if parent.kind() == SymbolKind::Struct {
                        parent.add_dependency(current_symbol);
                        self.propagate_child_dependencies(parent, current_symbol);
                    }
                }

                for child_id in node.children() {
                    let child = self.unit.hir_node(*child_id);
                    if child.kind_id() == LangPython::parameters {
                        self.add_parameter_type_dependencies(&child);
                        break;
                    }
                }
            }

            self.visit_children(&node);
            self.scopes.pop_until(depth);
        } else {
            self.visit_children(&node);
        }
    }

    fn visit_class_definition(&mut self, node: HirNode<'tcx>) {
        let name_node = match node.opt_child_by_field(self.unit, LangPython::field_name) {
            Some(n) => n,
            None => {
                self.visit_children(&node);
                return;
            }
        };

        let ident = match name_node.as_ident() {
            Some(id) => id,
            None => {
                self.visit_children(&node);
                return;
            }
        };

        let key = self.interner().intern(&ident.name);
        let mut symbol = self.lookup_symbol_suffix(&[key], Some(SymbolKind::Struct));

        if let Some(scope) = self.unit.opt_get_scope(node.hir_id()) {
            // If symbol not found by lookup, get it from the scope
            if symbol.is_none() {
                symbol = scope.symbol();
            }

            let depth = self.scopes.depth();
            self.scopes.push_with_symbol(scope, symbol);

            if let Some(current_symbol) = self.current_symbol() {
                self.add_base_class_dependencies(&node, current_symbol);
                for import_symbol in &self.module_imports {
                    current_symbol.add_dependency(import_symbol);
                }
            }

            self.visit_children(&node);
            self.scopes.pop_until(depth);
        } else {
            self.visit_children(&node);
        }
    }

    fn visit_decorated_definition(&mut self, node: HirNode<'tcx>) {
        self.visit_decorated_def(&node);
    }

    fn visit_block(&mut self, node: HirNode<'tcx>) {
        self.visit_children_scope(&node, None);
    }

    fn visit_call(&mut self, node: HirNode<'tcx>) {
        // Delegate to the existing visit_call method
        self.visit_call_impl(&node);
    }

    fn visit_assignment(&mut self, node: HirNode<'tcx>) {
        if let Some(type_node) = node.opt_child_by_field(self.unit, LangPython::field_type) {
            self.add_type_dependencies(&type_node);
        } else {
            for child_id in node.children() {
                let child = self.unit.hir_node(*child_id);
                if child.kind_id() == LangPython::type_node {
                    self.add_type_dependencies(&child);
                }
            }
        }

        self.visit_children(&node);
    }

    fn visit_import_statement(&mut self, node: HirNode<'tcx>) {
        let content = self.unit.file().content();
        let ts_node = node.inner_ts_node();
        let mut cursor = ts_node.walk();

        for child in ts_node.children(&mut cursor) {
            match child.kind() {
                "dotted_name" | "identifier" => {
                    if let Ok(text) = child.utf8_text(content) {
                        self.record_import_path(text);
                    }
                }
                "aliased_import" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        if let Ok(text) = name_node.utf8_text(content) {
                            self.record_import_path(text);
                        }
                    }
                }
                _ => {}
            }
        }

        self.visit_children(&node);
    }

    fn visit_import_from(&mut self, node: HirNode<'tcx>) {
        let content = self.unit.file().content();
        let ts_node = node.inner_ts_node();
        let mut cursor = ts_node.walk();

        for child in ts_node.children(&mut cursor) {
            match child.kind() {
                "dotted_name" | "identifier" => {
                    if let Ok(text) = child.utf8_text(content) {
                        self.record_import_path(text);
                    }
                }
                "aliased_import" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        if let Ok(text) = name_node.utf8_text(content) {
                            self.record_import_path(text);
                        }
                    }
                }
                _ => {}
            }
        }

        self.visit_children(&node);
    }

    fn visit_unknown(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }
}

pub fn bind_symbols<'tcx>(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>) -> BindingResult {
    let mut binder = SymbolBinder::new(unit, globals);

    if let Some(file_start_id) = unit.file_start_hir_id() {
        if let Some(root) = unit.opt_hir_node(file_start_id) {
            binder.visit_children(&root);
        }
    }

    BindingResult {
        calls: binder.calls,
    }
}
