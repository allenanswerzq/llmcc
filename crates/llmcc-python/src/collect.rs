use std::path::{Path, PathBuf};

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::SymbolKind;

use llmcc_descriptor::{
    CallDescriptor, CallKind, CallTarget, DescriptorTrait, TypeExpr, VariableScope, LANGUAGE_PYTHON,
};
use llmcc_resolver::{
    collect_symbols_batch, CallCollection, ClassCollection, CollectedSymbols, CollectionResult,
    CollectorCore, FunctionCollection, ImportCollection, SymbolSpec, VariableCollection,
};

use crate::describe::PythonDescriptor;
use crate::token::{AstVisitorPython, LangPython};

#[derive(Debug)]
struct DeclCollector<'tcx> {
    core: CollectorCore<'tcx>,
    functions: FunctionCollection,
    classes: ClassCollection,
    variables: VariableCollection,
    imports: ImportCollection,
    calls: CallCollection,
}

impl<'tcx> DeclCollector<'tcx> {
    fn new(unit: CompileUnit<'tcx>) -> Self {
        Self {
            core: CollectorCore::new(unit),
            functions: FunctionCollection::default(),
            classes: ClassCollection::default(),
            variables: VariableCollection::default(),
            imports: ImportCollection::default(),
            calls: CallCollection::default(),
        }
    }

    fn unit(&self) -> CompileUnit<'tcx> {
        self.core.unit()
    }

    fn parent_symbol(&self) -> Option<&SymbolSpec> {
        self.core.parent_symbol()
    }

    fn visit_children(&mut self, node: &HirNode<'tcx>) {
        for child_id in node.children() {
            let child = self.unit().hir_node(*child_id);
            self.visit_node(child);
        }
    }

    fn classify_symbol_call(&self, name: &str) -> CallKind {
        if self
            .core
            .symbols()
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
        let scope_idx = self.core.ensure_scope(owner);

        let unit = self.unit();
        let mut raw_path = unit.file_path().map(PathBuf::from);
        if raw_path.is_none() {
            if let Some(fallback) = unit.file().path() {
                raw_path = Some(PathBuf::from(fallback));
            }
        }

        let path = raw_path
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

        let (symbol_idx, _) =
            self.core
                .upsert_symbol_with_fqn(owner, &name, SymbolKind::Module, true, &fqn);

        self.core
            .set_scope_owner_symbol(scope_idx, Some(symbol_idx));
        Some(symbol_idx)
    }

    fn create_new_symbol(
        &mut self,
        node: &HirNode<'tcx>,
        field_id: u16,
        global: bool,
        kind: SymbolKind,
    ) -> Option<(usize, String, String)> {
        let ident_node = node.opt_child_by_field(self.unit(), field_id)?;
        let ident = ident_node.as_ident()?;
        let name = ident.name.clone();
        let owner = node.hir_id();
        let (symbol_idx, fqn) = self.core.upsert_symbol(owner, &name, kind, global);
        Some((symbol_idx, name, fqn))
    }

    fn apply_call_kind_hint(&self, descriptor: &mut CallDescriptor) {
        if let CallTarget::Symbol(symbol) = &mut descriptor.target {
            symbol.kind = self.classify_symbol_call(&symbol.name);
        }
    }

    fn extract_assignment_type(&self, node: &HirNode<'tcx>) -> Option<String> {
        if let Some(type_node) = node.opt_child_by_field(self.unit(), LangPython::field_type) {
            let text = self.unit().get_text(
                type_node.inner_ts_node().start_byte(),
                type_node.inner_ts_node().end_byte(),
            );
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }

        let assignment_text = self.unit().get_text(
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

    fn finish(self) -> CollectedSymbols {
        let DeclCollector {
            core,
            functions,
            classes,
            variables,
            imports,
            calls,
        } = self;

        core.finish(CollectionResult {
            functions,
            classes,
            variables,
            imports,
            calls,
            ..CollectionResult::default()
        })
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
    type ScopedSymbol = usize;

    fn unit(&self) -> CompileUnit<'tcx> {
        self.unit()
    }

    fn visit_children_scope(&mut self, node: &HirNode<'tcx>, symbol: Option<Self::ScopedSymbol>) {
        let owner = node.hir_id();
        let scope_idx = self.core.ensure_scope(owner);
        if let Some(sym_idx) = symbol {
            self.core.set_scope_owner_symbol(scope_idx, Some(sym_idx));
        }

        self.core.push_scope(scope_idx);
        self.visit_children(node);
        self.core.pop_scope();
    }

    fn visit_source_file(&mut self, node: HirNode<'tcx>) {
        let module_symbol = self.ensure_module_symbol(&node);
        self.visit_children_scope(&node, module_symbol);
    }

    fn visit_call(&mut self, node: HirNode<'tcx>) {
        if let Some(mut descriptor) = PythonDescriptor::build_call(self.unit(), &node) {
            self.apply_call_kind_hint(&mut descriptor);
            self.calls.add(node.hir_id(), descriptor);
        }
        self.visit_children(&node);
    }

    fn visit_function_definition(&mut self, node: HirNode<'tcx>) {
        if let Some((symbol_idx, _name, fqn)) =
            self.create_new_symbol(&node, LangPython::field_name, true, SymbolKind::Function)
        {
            if let Some(mut func) = PythonDescriptor::build_function(self.unit(), &node) {
                func.fqn = Some(fqn);
                self.functions.add(node.hir_id(), func);
            }
            self.visit_children_scope(&node, Some(symbol_idx));
        }
    }

    fn visit_class_definition(&mut self, node: HirNode<'tcx>) {
        if let Some((symbol_idx, _name, fqn)) =
            self.create_new_symbol(&node, LangPython::field_name, true, SymbolKind::Struct)
        {
            if let Some(mut class) = PythonDescriptor::build_impl(self.unit(), &node) {
                class.fqn = Some(fqn);
                self.classes.add(node.hir_id(), class);
            }
            self.visit_children_scope(&node, Some(symbol_idx));
        }
    }

    fn visit_decorated_definition(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }

    fn visit_import_statement(&mut self, node: HirNode<'tcx>) {
        if let Some(descriptor) = PythonDescriptor::build_import(self.unit(), &node) {
            self.imports.add(node.hir_id(), descriptor);
        }
    }

    fn visit_import_from(&mut self, node: HirNode<'tcx>) {
        if let Some(descriptor) = PythonDescriptor::build_import(self.unit(), &node) {
            self.imports.add(node.hir_id(), descriptor);
        }
    }

    fn visit_assignment(&mut self, node: HirNode<'tcx>) {
        if let Some((_symbol_idx, name, fqn)) =
            self.create_new_symbol(&node, LangPython::field_left, false, SymbolKind::Variable)
        {
            let scope = match self.parent_symbol().map(|spec| spec.kind) {
                Some(SymbolKind::Function) => VariableScope::Function,
                Some(SymbolKind::Struct) => VariableScope::Class,
                Some(SymbolKind::Module) => VariableScope::Module,
                _ => VariableScope::Unknown,
            };

            if let Some(mut var) = PythonDescriptor::build_variable(self.unit(), &node) {
                var.fqn = Some(fqn);
                var.name = name;
                var.scope = scope;
                if var.type_annotation.is_none() {
                    if let Some(annotation) = self.extract_assignment_type(&node) {
                        var.type_annotation = Some(TypeExpr::opaque(LANGUAGE_PYTHON, annotation));
                    }
                }
                self.variables.add(node.hir_id(), var);
            }
        }

        self.visit_children(&node);
    }

    fn visit_unknown(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }
}

pub fn collect_symbols<'tcx>(unit: CompileUnit<'tcx>) -> CollectedSymbols {
    let (collected, total_time, visit_time) = collect_symbols_batch(
        unit,
        DeclCollector::new,
        |collector, node| collector.visit_node(node),
        DeclCollector::finish,
    );

    if total_time.as_millis() > 10 {
        let result = &collected.result;
        tracing::trace!(
            "[COLLECT][python] File {:?}: total={:.2}ms, visit={:.2}ms, funcs={}, classes={}, vars={}, imports={}, calls={}",
            unit.file_path().unwrap_or("unknown"),
            total_time.as_secs_f64() * 1000.0,
            visit_time.as_secs_f64() * 1000.0,
            result.functions.len(),
            result.classes.len(),
            result.variables.len(),
            result.imports.len(),
            result.calls.len(),
        );
    }

    collected
}
