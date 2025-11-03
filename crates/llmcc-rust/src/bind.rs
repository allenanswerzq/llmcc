use std::ptr;

use llmcc_core::context::CompileUnit;
use llmcc_core::interner::InternedStr;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{Scope, ScopeStack, Symbol, SymbolKind};
use llmcc_descriptor::DescriptorTrait;

use crate::describe::RustDescriptor;
use crate::describe::{CallKind, CallTarget, TypeExpr};
use crate::token::{AstVisitorRust, LangRust};
/// `SymbolBinder` connects symbols with the items they reference so that later
/// stages (or LLM consumers) can reason about dependency relationships.
#[derive(Debug)]
struct SymbolBinder<'tcx, 'a> {
    unit: CompileUnit<'tcx>,
    scopes: ScopeStack<'tcx>,
    collection: &'a crate::CollectionResult,
}

impl<'tcx, 'a> SymbolBinder<'tcx, 'a> {
    pub fn new(
        unit: CompileUnit<'tcx>,
        globals: &'tcx Scope<'tcx>,
        collection: &'a crate::CollectionResult,
    ) -> Self {
        let mut scopes = ScopeStack::new(&unit.cc.arena, &unit.cc.interner, &unit.cc.symbol_map);
        scopes.push(globals);

        Self {
            unit,
            scopes,
            collection,
        }
    }

    fn interner(&self) -> &llmcc_core::interner::InternPool {
        self.unit.interner()
    }

    fn current_symbol(&self) -> Option<&'tcx Symbol> {
        self.scopes.scoped_symbol()
    }

    fn visit_children_scope(&mut self, node: HirNode<'tcx>, symbol: Option<&'tcx Symbol>) {
        let depth = self.scopes.depth();
        if let Some(symbol) = symbol {
            if let Some(parent) = self.scopes.scoped_symbol() {
                parent.add_dependency(symbol);
            }
        }

        // NOTE: scope should already be created during symbol collection, here we just
        // follow the tree structure again
        let scope = self.unit.opt_get_scope(node.hir_id());

        if let Some(scope) = scope {
            self.scopes.push_with_symbol(scope, symbol);
            self.visit_children(&node);
            self.scopes.pop_until(depth);
        } else {
            self.visit_children(&node);
        }
    }

    fn lookup_symbol_suffix(
        &mut self,
        suffix: &[InternedStr],
        kind: Option<SymbolKind>,
    ) -> Option<&'tcx Symbol> {
        let file_index = self.unit.index;
        self.lookup_in_local_scopes(suffix, kind, Some(file_index))
            .or_else(|| self.lookup_in_local_scopes(suffix, kind, None))
            .or_else(|| self.lookup_in_global_scope(suffix, kind, Some(file_index)))
            .or_else(|| self.lookup_in_global_scope(suffix, kind, None))
    }

    fn find_symbol_from_field(
        &mut self,
        node: &HirNode<'tcx>,
        field_id: u16,
        expected: SymbolKind,
    ) -> Option<&'tcx Symbol> {
        let child = node.opt_child_by_field(self.unit, field_id)?;
        let ident = child.as_ident()?;
        let key = self.interner().intern(&ident.name);
        self.lookup_symbol_suffix(&[key], Some(expected))
    }

    fn add_symbol_relation(&mut self, symbol: Option<&'tcx Symbol>) {
        if let (Some(current), Some(target)) = (self.current_symbol(), symbol) {
            current.add_dependency(target);
        }
    }

    fn add_symbol_relation_by_field(&mut self, node: &HirNode<'tcx>, field_id: u16) {
        let symbol = self.find_symbol_from_field(node, field_id, SymbolKind::EnumVariant);
        self.add_symbol_relation(symbol);
    }

    fn resolve_symbol(
        &mut self,
        segments: &[String],
        kind: Option<SymbolKind>,
    ) -> Option<&'tcx Symbol> {
        if segments.is_empty() {
            return None;
        }

        let suffix: Vec<_> = segments
            .iter()
            .rev()
            .map(|segment| self.interner().intern(segment))
            .collect();

        self.lookup_symbol_suffix(&suffix, kind)
    }

    fn resolve_symbol_with_priority(
        &mut self,
        segments: &[String],
        preferred_kinds: &[SymbolKind],
    ) -> Option<&'tcx Symbol> {
        if segments.is_empty() {
            return None;
        }

        for kind in preferred_kinds {
            if let Some(symbol) = self.resolve_symbol(segments, Some(*kind)) {
                return Some(symbol);
            }
        }

        self.resolve_symbol(segments, None)
    }

    fn resolve_symbol_method(&mut self, method: &str) -> Option<&'tcx Symbol> {
        let direct_segments = vec![method.to_string()];
        if let Some(symbol) =
            self.resolve_symbol_with_priority(&direct_segments, &[SymbolKind::Function])
        {
            return Some(symbol);
        }

        let owner_fqns: Vec<String> = self
            .scopes
            .iter()
            .rev()
            .filter_map(|scope| scope.symbol().map(|symbol| symbol.fqn_name.read().clone()))
            .collect();

        for fqn in owner_fqns {
            if fqn.is_empty() {
                continue;
            }
            let mut segments: Vec<String> = fqn
                .split("::")
                .filter(|segment| !segment.is_empty())
                .map(|segment| segment.to_string())
                .collect();
            segments.push(method.to_string());
            if let Some(symbol) =
                self.resolve_symbol_with_priority(&segments, &[SymbolKind::Function])
            {
                return Some(symbol);
            }
        }

        None
    }

    fn resolve_symbol_type_expr(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let ts_node = node.inner_ts_node();
        let expr = RustDescriptor::build_type_expr(self.unit, ts_node);
        let segments = expr.path_segments().map(|segments| segments.to_vec())?;
        self.resolve_symbol_with_priority(&segments, &[SymbolKind::Struct, SymbolKind::Enum])
    }

    /// Traverse a type expression and collect every symbol the type mentions.
    ///
    /// To build intuition, consider a function that returns `Option<Result<MyStruct, MyError>>`.
    /// This helper ensures all three symbols (`Option`, `Result`, `MyStruct`, `MyError`) end up in
    /// the `symbols` vector so the binder can record dependencies.
    fn resolve_symbols_from_type_expr(&mut self, expr: &TypeExpr, symbols: &mut Vec<&'tcx Symbol>) {
        match expr {
            // Simple path types like `Foo` or qualified paths such as `crate::foo::Bar`.
            // We first favor struct/enum resolution, but fall back to a generic lookup so we still
            // record aliases or type aliases.
            TypeExpr::Path { segments, generics } => {
                let symbol = self.resolve_symbol_with_priority(
                    segments,
                    &[SymbolKind::Struct, SymbolKind::Enum],
                );
                if let Some(symbol) = symbol {
                    if !symbols.iter().any(|existing| existing.id == symbol.id) {
                        symbols.push(symbol);
                    }
                }

                // Example: `Option<Result<MyStruct, MyError>>` – recurse into each generic argument
                // (`Result`, `MyStruct`, `MyError`).
                for generic in generics {
                    self.resolve_symbols_from_type_expr(generic, symbols);
                }
            }

            // `&T` or `&mut T` still reference `T`, so we just recurse into the inner type.
            TypeExpr::Reference { inner, .. } => {
                self.resolve_symbols_from_type_expr(inner, symbols);
            }

            // Tuple types like `(Foo, Option<Bar>)` contain multiple elements; visit each one.
            TypeExpr::Tuple(items) => {
                for item in items {
                    self.resolve_symbols_from_type_expr(item, symbols);
                }
            }

            // Callable types (e.g., `fn(Foo, &Bar) -> Baz`) carry parameter and optional result types.
            // We walk the parameters and the return type if present.
            TypeExpr::Callable { parameters, result } => {
                for parameter in parameters {
                    self.resolve_symbols_from_type_expr(parameter, symbols);
                }
                if let Some(result) = result.as_deref() {
                    self.resolve_symbols_from_type_expr(result, symbols);
                }
            }

            // `impl Trait`, opaque types, or unknown nodes do not map cleanly to named symbols.
            // We currently skip them, but we keep the branch explicit for future handling.
            TypeExpr::ImplTrait { .. } | TypeExpr::Opaque { .. } | TypeExpr::Unknown(_) => {}
        }
    }

    fn lookup_in_local_scopes(
        &self,
        suffix: &[InternedStr],
        kind: Option<SymbolKind>,
        file: Option<usize>,
    ) -> Option<&'tcx Symbol> {
        let global_scope = self.scopes.iter().next();
        for scope in self.scopes.iter().rev() {
            if let Some(global) = global_scope {
                if ptr::eq(scope, global) {
                    continue;
                }
            }
            let symbols = scope.lookup_suffix_symbols(suffix);
            if let Some(symbol) = self.select_matching_symbol(&symbols, kind, file) {
                return Some(symbol);
            }
        }
        None
    }

    fn lookup_in_global_scope(
        &self,
        suffix: &[InternedStr],
        kind: Option<SymbolKind>,
        file: Option<usize>,
    ) -> Option<&'tcx Symbol> {
        if let Some(global_scope) = self.scopes.iter().next() {
            let symbols = global_scope.lookup_suffix_symbols(suffix);
            self.select_matching_symbol(&symbols, kind, file)
        } else {
            None
        }
    }

    /// Pick the most specific match from a list of candidates.
    ///
    /// Selection priority:
    /// 1. A candidate that matches both `kind` and `file` (when requested).
    /// 2. Any candidate that matches the requested `kind`.
    /// 3. Any candidate that matches the requested `file`.
    /// 4. The first candidate in declaration order.
    fn select_matching_symbol(
        &self,
        candidates: &[&'tcx Symbol],
        kind: Option<SymbolKind>,
        file: Option<usize>,
    ) -> Option<&'tcx Symbol> {
        if candidates.is_empty() {
            return None;
        }

        // Helper that checks whether a candidate matches the requested kind and/or file.
        let matches_all = |symbol: &&'tcx Symbol| -> bool {
            let kind_ok = kind.map_or(true, |expected| symbol.kind() == expected);
            let file_ok = file.map_or(true, |expected| symbol.unit_index() == Some(expected));
            kind_ok && file_ok
        };

        // Helper that checks only the requested kind, ignoring file.
        let matches_kind_only = |symbol: &&'tcx Symbol| -> bool {
            kind.map_or(true, |expected| symbol.kind() == expected)
        };

        // Helper that checks only the requested file, ignoring kind.
        let matches_file_only = |symbol: &&'tcx Symbol| -> bool {
            file.map_or(true, |expected| symbol.unit_index() == Some(expected))
        };

        // Prefer symbols that satisfy every requested filter.
        if let Some(symbol) = candidates.iter().find(matches_all) {
            return Some(*symbol);
        }

        // If we could not satisfy both filters together, fall back to matching by kind.
        if kind.is_some() {
            if let Some(symbol) = candidates.iter().find(matches_kind_only) {
                return Some(*symbol);
            }
            // When a specific kind was requested but none matched, stop searching.
            return None;
        }

        // No kind restriction; if a file was requested, honour it before defaulting to the first match.
        if file.is_some() {
            if let Some(symbol) = candidates.iter().find(matches_file_only) {
                return Some(*symbol);
            }
        }

        candidates.first().copied()
    }

    fn add_call_target_dependencies(&mut self, target: &CallTarget) {
        match target {
            CallTarget::Symbol(symbol) => {
                let mut segments = symbol.qualifiers.clone();
                segments.push(symbol.name.clone());

                match symbol.kind {
                    CallKind::Method => {
                        if let Some(method_symbol) = self.resolve_symbol_method(&symbol.name) {
                            self.add_symbol_relation(Some(method_symbol));
                        }
                    }
                    CallKind::Constructor => {
                        if let Some(struct_symbol) =
                            self.resolve_symbol(&segments, Some(SymbolKind::Struct))
                        {
                            self.add_symbol_relation(Some(struct_symbol));
                        } else if let Some(enum_symbol) =
                            self.resolve_symbol(&segments, Some(SymbolKind::Enum))
                        {
                            self.add_symbol_relation(Some(enum_symbol));
                        }
                    }
                    CallKind::Function | CallKind::Macro | CallKind::Unknown => {
                        if let Some(function_symbol) =
                            self.resolve_symbol(&segments, Some(SymbolKind::Function))
                        {
                            self.add_symbol_relation(Some(function_symbol));
                            if segments.len() > 1 {
                                self.add_type_dependency_for_segments(&segments);
                            }
                        } else if !segments.is_empty() {
                            self.add_type_dependency_for_segments(&segments);
                        }
                    }
                }
            }
            CallTarget::Chain(chain) => {
                if let Some(symbol) = self.resolve_path_text(&chain.root) {
                    self.add_symbol_relation(Some(symbol));
                } else {
                    let segments: Vec<String> = chain
                        .root
                        .split("::")
                        .filter(|segment| !segment.is_empty())
                        .map(|segment| segment.trim().to_string())
                        .collect();
                    if segments.len() > 1 {
                        self.add_type_dependency_for_segments(&segments);
                    }
                }

                for segment in &chain.segments {
                    match segment.kind {
                        CallKind::Method | CallKind::Function => {
                            if let Some(symbol) = self.resolve_symbol_method(&segment.name) {
                                self.add_symbol_relation(Some(symbol));
                            }
                        }
                        CallKind::Constructor => {
                            if let Some(symbol) = self.resolve_symbol(
                                std::slice::from_ref(&segment.name),
                                Some(SymbolKind::Struct),
                            ) {
                                self.add_symbol_relation(Some(symbol));
                            }
                        }
                        CallKind::Macro | CallKind::Unknown => {}
                    }
                }
            }
            CallTarget::Dynamic { .. } => {}
        }
    }

    fn resolve_path_text(&mut self, text: &str) -> Option<&'tcx Symbol> {
        if text.is_empty() {
            return None;
        }
        let cleaned = text.split('<').next().unwrap_or(text);
        let segments: Vec<String> = cleaned
            .split("::")
            .map(|segment| segment.trim().to_string())
            .filter(|segment| !segment.is_empty())
            .collect();
        self.resolve_symbol_with_priority(&segments, &[])
    }

    fn add_type_dependency_for_segments(&mut self, segments: &[String]) {
        if segments.len() <= 1 {
            return;
        }

        let base_segments = &segments[..segments.len() - 1];
        let base_segments: Vec<String> = base_segments.to_vec();
        if let Some(sym) = self
            .resolve_symbol_with_priority(&base_segments, &[SymbolKind::Struct, SymbolKind::Enum])
        {
            self.add_symbol_relation(Some(sym));
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

impl<'tcx> AstVisitorRust<'tcx> for SymbolBinder<'tcx, '_> {
    fn unit(&self) -> CompileUnit<'tcx> {
        self.unit
    }

    fn visit_source_file(&mut self, node: HirNode<'tcx>) {
        self.visit_children_scope(node, None);
    }

    fn visit_mod_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.find_symbol_from_field(&node, LangRust::field_name, SymbolKind::Module);
        self.visit_children_scope(node, symbol);
    }

    fn visit_struct_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.find_symbol_from_field(&node, LangRust::field_name, SymbolKind::Struct);

        if let Some(&descriptor_idx) = self.collection.struct_map.get(&node.hir_id()) {
            if let Some(struct_descriptor) = self.collection.structs.get(descriptor_idx) {
                for field in &struct_descriptor.fields {
                    if let Some(type_expr) = field.type_annotation.as_ref() {
                        let mut symbols = Vec::new();
                        self.resolve_symbols_from_type_expr(type_expr, &mut symbols);
                        for type_symbol in symbols {
                            self.add_symbol_relation(Some(type_symbol));
                        }
                    }
                }
            }
        }

        self.visit_children_scope(node, symbol);
    }

    fn visit_enum_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.find_symbol_from_field(&node, LangRust::field_name, SymbolKind::Enum);
        self.visit_children_scope(node, symbol);
    }

    fn visit_function_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.find_symbol_from_field(&node, LangRust::field_name, SymbolKind::Function);
        let parent_symbol = self.current_symbol();
        if let Some(func_symbol) = symbol {
            if let Some(&descriptor_idx) = self.collection.function_map.get(&node.hir_id()) {
                if let Some(return_type) = self.collection.functions[descriptor_idx]
                    .return_type
                    .as_ref()
                {
                    let mut symbols = Vec::new();
                    self.resolve_symbols_from_type_expr(return_type, &mut symbols);
                    for return_type_sym in symbols {
                        func_symbol.add_dependency(return_type_sym);
                    }
                }
            }

            // If this function is inside an impl block, it depends on the impl's target struct/enum
            // The current_symbol() when visiting impl children is the target struct/enum
            if let Some(parent_symbol) = parent_symbol {
                let kind = parent_symbol.kind();
                if matches!(kind, SymbolKind::Struct | SymbolKind::Enum) {
                    func_symbol.add_dependency(parent_symbol);
                }
            }
        }

        self.visit_children_scope(node, symbol);

        if let (Some(parent_symbol), Some(func_symbol)) = (parent_symbol, symbol) {
            // When visiting `impl Foo { ... }`, `parent_symbol` refers to the synthetic
            // impl symbol and `func_symbol` is the method we just bound. We copy the
            // method’s dependencies back onto the impl so callers that link against the
            // impl symbol (rather than the individual method) still receive transitive
            // edges, e.g. `impl Foo` depends on `Foo` if the method returns that type.
            if matches!(parent_symbol.kind(), SymbolKind::Impl) {
                self.propagate_child_dependencies(parent_symbol, func_symbol);
            }
        }
    }

    fn visit_impl_item(&mut self, node: HirNode<'tcx>) {
        let impl_descriptor = self
            .collection
            .impl_map
            .get(&node.hir_id())
            .and_then(|&idx| self.collection.impls.get(idx));

        // Use the descriptor’s fully-qualified name to locate the target type symbol. The
        // descriptor already normalized nested paths (e.g., `crate::foo::Bar`).
        let symbol = impl_descriptor.and_then(|descriptor| {
            descriptor.impl_target_fqn.as_ref().and_then(|fqn| {
                let segments: Vec<String> = fqn
                    .split("::")
                    .map(|segment| segment.trim().to_string())
                    .filter(|segment| !segment.is_empty())
                    .collect();
                if segments.is_empty() {
                    None
                } else {
                    self.resolve_symbol_with_priority(
                        &segments,
                        &[SymbolKind::Struct, SymbolKind::Enum, SymbolKind::Trait],
                    )
                }
            })
        });

        // If we know both the descriptor and the target symbol, record trait → type dependencies.
        //    Example: `impl Display for Foo` should establish Display → Foo so trait queries reach Foo.
        if let (Some(descriptor), Some(target_symbol)) = (impl_descriptor, symbol) {
            // base_types is the traits being implemented
            for base in &descriptor.base_types {
                if let Some(segments) = base.path_segments().map(|segments| segments.to_vec()) {
                    if let Some(trait_symbol) = self
                        .resolve_symbol(&segments, Some(SymbolKind::Trait))
                        .or_else(|| self.resolve_symbol(&segments, None))
                    {
                        trait_symbol.add_dependency(target_symbol);
                    }
                }
            }
        } else {
            tracing::warn!("failed to build descriptor for impl");
        }

        self.visit_children_scope(node, symbol);
    }

    fn visit_trait_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.find_symbol_from_field(&node, LangRust::field_name, SymbolKind::Trait);
        self.visit_children_scope(node, symbol);
    }

    fn visit_block(&mut self, node: HirNode<'tcx>) {
        self.visit_children_scope(node, None);
    }

    fn visit_let_declaration(&mut self, node: HirNode<'tcx>) {
        if let Some(&descriptor_idx) = self.collection.variable_map.get(&node.hir_id()) {
            if let Some(type_expr) = self.collection.variables[descriptor_idx]
                .type_annotation
                .as_ref()
            {
                let mut symbols = Vec::new();
                self.resolve_symbols_from_type_expr(type_expr, &mut symbols);
                for type_symbol in symbols {
                    self.add_symbol_relation(Some(type_symbol));
                }
            }
        }

        self.visit_children(&node);
    }

    fn visit_parameter(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }

    fn visit_const_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.find_symbol_from_field(&node, LangRust::field_name, SymbolKind::Const);
        self.visit_children_scope(node, symbol);
    }

    fn visit_static_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.find_symbol_from_field(&node, LangRust::field_name, SymbolKind::Const);
        self.visit_children_scope(node, symbol);
    }

    fn visit_enum_variant(&mut self, node: HirNode<'tcx>) {
        self.add_symbol_relation_by_field(&node, LangRust::field_name);
        self.visit_children(&node);
    }

    fn visit_call_expression(&mut self, node: HirNode<'tcx>) {
        let call = self
            .collection
            .call_map
            .get(&node.hir_id())
            .and_then(|&idx| self.collection.calls.get(idx));
        if let Some(descriptor) = call {
            self.add_call_target_dependencies(&descriptor.target);
        }
        self.visit_children(&node);
    }

    fn visit_scoped_identifier(&mut self, node: HirNode<'tcx>) {
        self.visit_type_identifier(node);
    }

    fn visit_type_identifier(&mut self, node: HirNode<'tcx>) {
        if let Some(symbol) = self.resolve_symbol_type_expr(&node) {
            // Skip struct/enum dependencies if this identifier is followed by parentheses (bare constructor call).
            // This is detected by checking if the parent is a call_expression.
            if matches!(symbol.kind(), SymbolKind::Struct | SymbolKind::Enum) {
                // Check parent - if it's a call_expression, this might be a bare constructor call
                if let Some(parent_id) = node.parent() {
                    let parent = self.unit.hir_node(parent_id);
                    if parent.kind_id() == LangRust::call_expression {
                        // Parent is a call expression - skip adding this dependency
                        // add_call_target_dependencies will handle it
                        self.visit_children(&node);
                        return;
                    }
                }
            }
            self.add_symbol_relation(Some(symbol));
        }
        self.visit_children(&node);
    }

    fn visit_identifier(&mut self, node: HirNode<'tcx>) {
        let ident = node.as_ident().unwrap();
        let key = self.interner().intern(&ident.name);

        let symbol = if let Some(local) = self.scopes.find_symbol_local(&ident.name) {
            Some(local)
        } else {
            self.scopes
                .find_scoped_suffix_with_filters(&[key], None, Some(self.unit.index))
                .or_else(|| {
                    self.scopes
                        .find_scoped_suffix_with_filters(&[key], None, None)
                })
                .or_else(|| {
                    self.scopes
                        .find_global_suffix_with_filters(&[key], None, Some(self.unit.index))
                })
                .or_else(|| {
                    self.scopes
                        .find_global_suffix_with_filters(&[key], None, None)
                })
        };

        if let Some(sym) = symbol {
            if !matches!(sym.kind(), SymbolKind::Struct | SymbolKind::Enum) {
                self.add_symbol_relation(Some(sym));
            }
        }
    }

    fn visit_unknown(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }
}

pub fn bind_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    globals: &'tcx Scope<'tcx>,
    collection: &crate::CollectionResult,
) {
    let root = unit.file_start_hir_id().unwrap();
    let node = unit.hir_node(root);
    let mut binder = SymbolBinder::new(unit, globals, collection);
    binder.visit_node(node);
}
