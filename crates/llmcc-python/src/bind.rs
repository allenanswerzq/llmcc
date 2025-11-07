use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{Scope, ScopeStack, Symbol, SymbolKind};

use llmcc_descriptor::{CallChain, CallChainRoot, TypeExpr};
use llmcc_resolver::{BinderCore, CollectedSymbols, CollectionResult};

use crate::token::{AstVisitorPython, LangPython};

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
struct SymbolBinder<'tcx, 'a> {
    core: BinderCore<'tcx, 'a>,
    calls: Vec<CallBinding>,
    module_imports: Vec<&'tcx Symbol>,
}

impl<'tcx, 'a> SymbolBinder<'tcx, 'a> {
    pub fn new(
        unit: CompileUnit<'tcx>,
        globals: &'tcx Scope<'tcx>,
        collection: &'a CollectionResult,
    ) -> Self {
        Self {
            core: BinderCore::new(unit, globals, collection),
            calls: Vec::new(),
            module_imports: Vec::new(),
        }
    }

    fn unit(&self) -> CompileUnit<'tcx> {
        self.core.unit()
    }

    fn collection(&self) -> &'a CollectionResult {
        self.core.collection()
    }

    fn scopes(&self) -> &ScopeStack<'tcx> {
        self.core.scopes()
    }

    fn scopes_mut(&mut self) -> &mut ScopeStack<'tcx> {
        self.core.scopes_mut()
    }

    fn interner(&self) -> &llmcc_core::interner::InternPool {
        self.core.interner()
    }

    fn current_symbol(&self) -> Option<&'tcx Symbol> {
        self.core.current_symbol()
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
        let unit = self.unit();
        let scope = unit.alloc_scope(node.hir_id());
        if let Some(symbol) = scope.symbol() {
            return Some(symbol);
        }

        let raw_path = unit.file_path().or_else(|| unit.file().path());
        let path = raw_path
            .map(PathBuf::from)
            .and_then(|p| p.canonicalize().ok().or(Some(p)))
            .unwrap_or_else(|| PathBuf::from("__module__"));

        let segments = Self::module_segments_from_path(&path);
        let interner = unit.interner();

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
        let symbol = unit.cc.arena.alloc(symbol);
        symbol.set_kind(SymbolKind::Module);
        symbol.set_unit_index(unit.index);
        symbol.set_fqn(fqn, interner);

        unit.cc.symbol_map.write().insert(symbol.id, symbol);

        let _ = self.scopes_mut().insert_symbol(symbol, true);
        scope.set_symbol(Some(symbol));
        Some(symbol)
    }

    fn add_symbol_relation(&mut self, symbol: Option<&'tcx Symbol>) {
        self.core.add_symbol_dependency(symbol);

        let Some(target) = symbol else { return };
        let Some(current) = self.current_symbol() else {
            return;
        };

        match current.kind() {
            SymbolKind::Function => {
                let parent_class = self
                    .scopes()
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

        let target = self.core.lookup_segments_with_priority(
            segments,
            &[SymbolKind::Struct, SymbolKind::Enum, SymbolKind::Module],
            None,
        );

        self.add_symbol_relation(target);
    }

    fn record_decorator_dependency(&mut self, decorator: &str) {
        let base = decorator
            .split(|c: char| c == '(' || c.is_whitespace())
            .next()
            .unwrap_or(decorator)
            .trim();
        if base.is_empty() {
            return;
        }
        let segments: Vec<String> = base
            .split('.')
            .filter(|segment| !segment.is_empty())
            .map(|segment| segment.to_string())
            .collect();
        if segments.is_empty() {
            return;
        }

        self.record_segments_dependency(&segments);
    }

    fn record_type_repr_dependencies(&mut self, text: &str) {
        for segments in Self::segments_from_type_repr(text) {
            self.record_segments_dependency(&segments);
        }
    }

    fn add_type_expr_dependencies(&mut self, expr: &TypeExpr) {
        match expr {
            TypeExpr::Path { segments, generics } => {
                if !segments.is_empty() {
                    self.record_segments_dependency(segments);
                }
                for generic in generics {
                    self.add_type_expr_dependencies(generic);
                }
            }
            TypeExpr::Reference { inner, .. } => self.add_type_expr_dependencies(inner),
            TypeExpr::Tuple(items) => {
                for item in items {
                    self.add_type_expr_dependencies(item);
                }
            }
            TypeExpr::Callable { parameters, result } => {
                for parameter in parameters {
                    self.add_type_expr_dependencies(parameter);
                }
                if let Some(result) = result.as_deref() {
                    self.add_type_expr_dependencies(result);
                }
            }
            TypeExpr::ImplTrait { bounds } => self.record_type_repr_dependencies(bounds),
            TypeExpr::Opaque { repr, .. } | TypeExpr::Unknown(repr) => {
                self.record_type_repr_dependencies(repr)
            }
        }
    }

    fn segments_from_type_repr(text: &str) -> Vec<Vec<String>> {
        let mut seen = BTreeSet::new();
        let mut results = Vec::new();

        for token in text
            .split(|c: char| !c.is_alphanumeric() && c != '.' && c != '_')
            .map(str::trim)
            .filter(|token| !token.is_empty())
        {
            let cleaned = token.trim_matches('.');
            if cleaned.is_empty() {
                continue;
            }

            let parts: Vec<String> = cleaned
                .split('.')
                .filter(|segment| !segment.is_empty())
                .map(|segment| segment.to_string())
                .collect();

            if parts.is_empty() {
                continue;
            }

            let key = parts.join("::");
            if seen.insert(key) {
                results.push(parts);
            }
        }

        results
    }

    fn build_attribute_path(&mut self, node: &HirNode<'tcx>, out: &mut Vec<String>) {
        if node.kind_id() == LangPython::attribute {
            if let Some(object_node) =
                node.opt_child_by_field(self.unit(), LangPython::field_object)
            {
                self.build_attribute_path(&object_node, out);
            }
            if let Some(attr_node) =
                node.opt_child_by_field(self.unit(), LangPython::field_attribute)
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
                let child = self.unit().hir_node(*child_id);
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
            let child = self.unit().hir_node(*child_id);
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
        let normalized = path.replace("::", ".");
        let segments: Vec<String> = normalized
            .split('.')
            .filter(|segment| !segment.is_empty())
            .map(|segment| segment.trim().to_string())
            .collect();
        if segments.is_empty() {
            return;
        }

        let target = self.core.lookup_segments_with_priority(
            &segments,
            &[SymbolKind::Struct, SymbolKind::Enum, SymbolKind::Module],
            None,
        );

        self.add_symbol_relation(target);
    }

    fn process_call_descriptor(&mut self, descriptor: &llmcc_descriptor::CallDescriptor) {
        match &descriptor.target {
            llmcc_descriptor::CallTarget::Symbol(symbol) => {
                let mut segments = symbol.qualifiers.clone();
                segments.push(symbol.name.clone());
                if !self.handle_symbol_segments(&segments) {
                    self.handle_symbol_segments(std::slice::from_ref(&symbol.name));
                }
            }
            llmcc_descriptor::CallTarget::Chain(chain) => {
                if let Some(target) = self.resolve_method_from_chain(chain) {
                    self.add_symbol_relation(Some(target));
                    self.record_call_binding(target);
                } else if let Some(segment) = chain.segments.last() {
                    self.handle_symbol_segments(std::slice::from_ref(&segment.name));
                }
            }
            llmcc_descriptor::CallTarget::Dynamic { .. } => {}
        }
    }

    fn handle_symbol_segments(&mut self, segments: &[String]) -> bool {
        if segments.is_empty() {
            return false;
        }

        if let Some(target) = self.core.lookup_segments_with_priority(
            segments,
            &[SymbolKind::Function, SymbolKind::Struct],
            None,
        ) {
            self.add_symbol_relation(Some(target));
            if target.kind() == SymbolKind::Function {
                self.record_call_binding(target);
            }
            return true;
        }

        if let Some(target) = self.core.lookup_segments(segments, None, None) {
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
        let root_is_self = matches!(
            &chain.root,
            CallChainRoot::Expr(expr) if expr == "self"
        );

        if root_is_self {
            let class_fqn = self
                .scopes()
                .iter()
                .rev()
                .filter_map(|scope| scope.symbol())
                .find(|symbol| symbol.kind() == SymbolKind::Struct)
                .map(|symbol| symbol.fqn_name.read().clone());

            if let Some(class_fqn) = class_fqn {
                let method_fqn = format!("{}::{}", class_fqn, segment.name);
                let key = self.interner().intern(&method_fqn);
                return self
                    .core
                    .lookup_symbol_suffix(&[key], Some(SymbolKind::Function), None)
                    .or_else(|| self.core.lookup_symbol_suffix(&[key], None, None));
            }
        }

        None
    }

    fn add_base_class_dependencies(&mut self, node: &HirNode<'tcx>, class_symbol: &Symbol) {
        for child_id in node.children() {
            let child = self.unit().hir_node(*child_id);
            if child.kind_id() == LangPython::argument_list {
                for base_id in child.children() {
                    let base_node = self.unit().hir_node(*base_id);

                    if let Some(ident) = base_node.as_ident() {
                        let key = self.interner().intern(&ident.name);
                        if let Some(base_symbol) =
                            self.core
                                .lookup_symbol_suffix(&[key], Some(SymbolKind::Struct), None)
                        {
                            class_symbol.add_dependency(base_symbol);
                        }
                    } else if base_node.kind_id() == LangPython::attribute {
                        if let Some(attr_node) =
                            base_node.inner_ts_node().child_by_field_name("attribute")
                        {
                            let content = self.unit().file().content();
                            if let Ok(name) = attr_node.utf8_text(content) {
                                let key = self.interner().intern(name);
                                if let Some(base_symbol) = self.core.lookup_symbol_suffix(
                                    &[key],
                                    Some(SymbolKind::Struct),
                                    None,
                                ) {
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
            let child = self.unit().hir_node(*child_id);
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
            let child = self.unit().hir_node(*child_id);
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

            if let Some(dep_symbol) = self.unit().opt_get_symbol(dep_id) {
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

impl<'tcx> AstVisitorPython<'tcx> for SymbolBinder<'tcx, '_> {
    type ScopedSymbol = &'tcx Symbol;

    fn unit(&self) -> CompileUnit<'tcx> {
        self.core.unit()
    }

    fn visit_children_scope(&mut self, node: &HirNode<'tcx>, symbol: Option<Self::ScopedSymbol>) {
        let depth = self.scopes().depth();
        if let Some(symbol) = symbol {
            if let Some(parent) = self.current_symbol() {
                parent.add_dependency(symbol);
            }
        }

        let scope = self.unit().opt_get_scope(node.hir_id());
        if let Some(scope) = scope {
            self.scopes_mut().push_with_symbol(scope, symbol);
            self.visit_children(node);
            self.scopes_mut().pop_until(depth);
        } else {
            self.visit_children(node);
        }
    }

    fn visit_source_file(&mut self, node: HirNode<'tcx>) {
        self.module_imports.clear();
        let module_symbol = self.ensure_module_symbol(&node);
        self.visit_children_scope(&node, module_symbol);
    }

    fn visit_function_definition(&mut self, node: HirNode<'tcx>) {
        let name_node = match node.opt_child_by_field(self.unit(), LangPython::field_name) {
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
        let mut symbol = self
            .core
            .lookup_symbol_suffix(&[key], Some(SymbolKind::Function), None);

        // Get the parent symbol before pushing a new scope
        let parent_symbol = self.current_symbol();

        if let Some(scope) = self.unit().opt_get_scope(node.hir_id()) {
            // If symbol not found by lookup, get it from the scope
            if symbol.is_none() {
                symbol = scope.symbol();
            }

            let depth = self.scopes().depth();
            self.scopes_mut().push_with_symbol(scope, symbol);

            if let Some(current_symbol) = self.current_symbol() {
                // If parent is a class, class depends on method
                if let Some(parent) = parent_symbol {
                    if parent.kind() == SymbolKind::Struct {
                        parent.add_dependency(current_symbol);
                        self.propagate_child_dependencies(parent, current_symbol);
                    }
                }

                if let Some(descriptor) = self.collection().functions.find(node.hir_id()) {
                    if let Some(return_type) = descriptor.return_type.as_ref() {
                        self.add_type_expr_dependencies(return_type);
                    }

                    for parameter in &descriptor.parameters {
                        if let Some(type_expr) = parameter.type_hint.as_ref() {
                            self.add_type_expr_dependencies(type_expr);
                        }
                    }

                    for decorator in &descriptor.decorators {
                        self.record_decorator_dependency(decorator);
                    }
                } else {
                    for child_id in node.children() {
                        let child = self.unit().hir_node(*child_id);
                        if child.kind_id() == LangPython::parameters {
                            self.add_parameter_type_dependencies(&child);
                            break;
                        }
                    }
                }
            }

            self.visit_children(&node);
            self.scopes_mut().pop_until(depth);
        } else {
            self.visit_children(&node);
        }
    }

    fn visit_class_definition(&mut self, node: HirNode<'tcx>) {
        let name_node = match node.opt_child_by_field(self.unit(), LangPython::field_name) {
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
        let mut symbol = self
            .core
            .lookup_symbol_suffix(&[key], Some(SymbolKind::Struct), None);

        if let Some(scope) = self.unit().opt_get_scope(node.hir_id()) {
            // If symbol not found by lookup, get it from the scope
            if symbol.is_none() {
                symbol = scope.symbol();
            }

            let depth = self.scopes().depth();
            self.scopes_mut().push_with_symbol(scope, symbol);

            if let Some(current_symbol) = self.current_symbol() {
                if let Some(descriptor) = self.collection().classes.find(node.hir_id()) {
                    for base in &descriptor.base_types {
                        self.add_type_expr_dependencies(base);
                    }
                    for decorator in &descriptor.decorators {
                        self.record_decorator_dependency(decorator);
                    }
                } else {
                    self.add_base_class_dependencies(&node, current_symbol);
                }
                for import_symbol in &self.module_imports {
                    current_symbol.add_dependency(import_symbol);
                }
            }

            self.visit_children(&node);
            self.scopes_mut().pop_until(depth);
        } else {
            self.visit_children(&node);
        }
    }

    fn visit_decorated_definition(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }

    fn visit_block(&mut self, node: HirNode<'tcx>) {
        self.visit_children_scope(&node, None);
    }

    fn visit_call(&mut self, node: HirNode<'tcx>) {
        if let Some(descriptor) = self.collection().calls.find(node.hir_id()) {
            self.process_call_descriptor(descriptor);
        }
        self.visit_children(&node);
    }

    fn visit_assignment(&mut self, node: HirNode<'tcx>) {
        let mut handled = false;
        if let Some(descriptor) = self.collection().variables.find(node.hir_id()) {
            if let Some(type_expr) = descriptor.type_annotation.as_ref() {
                self.add_type_expr_dependencies(type_expr);
                handled = true;
            }
        }

        if !handled {
            if let Some(type_node) = node.opt_child_by_field(self.unit(), LangPython::field_type) {
                self.add_type_dependencies(&type_node);
            } else {
                for child_id in node.children() {
                    let child = self.unit().hir_node(*child_id);
                    if child.kind_id() == LangPython::type_node {
                        self.add_type_dependencies(&child);
                    }
                }
            }
        }

        self.visit_children(&node);
    }

    fn visit_import_statement(&mut self, node: HirNode<'tcx>) {
        if let Some(descriptor) = self.collection().imports.find(node.hir_id()) {
            self.record_import_path(&descriptor.source);
        }
        self.visit_children(&node);
    }

    fn visit_import_from(&mut self, node: HirNode<'tcx>) {
        if let Some(descriptor) = self.collection().imports.find(node.hir_id()) {
            self.record_import_path(&descriptor.source);
        }
        self.visit_children(&node);
    }

    fn visit_unknown(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }
}

pub fn bind_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    globals: &'tcx Scope<'tcx>,
    collection: &CollectedSymbols,
) -> BindingResult {
    let mut binder = SymbolBinder::new(unit, globals, &collection.result);

    if let Some(file_start_id) = unit.file_start_hir_id() {
        if let Some(root) = unit.opt_hir_node(file_start_id) {
            binder.visit_children(&root);
        }
    }

    BindingResult {
        calls: binder.calls,
    }
}
