// Allow `self` only used in recursion - common pattern for tree traversal
#![allow(clippy::only_used_in_recursion)]

use std::collections::HashMap;

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirIdent, HirNode, HirScope};
use llmcc_core::next_hir_id;
use llmcc_core::scope::{Scope, ScopeStack};
use llmcc_core::symbol::{ScopeId, SymKind, SymKindSet, Symbol};
use llmcc_resolver::{CollectorScopes, ResolverOption};

use crate::LangCpp;
use crate::token::AstVisitorCpp;

/// Check if a function is in a method context (parent is a class/struct)
fn is_method_context(parent: Option<&Symbol>) -> bool {
    parent.is_some_and(|p| {
        matches!(
            p.kind(),
            SymKind::Struct | SymKind::Trait | SymKind::UnresolvedType
        )
    })
}

#[derive(Debug)]
pub struct CollectorVisitor<'tcx> {
    scope_map: HashMap<ScopeId, &'tcx Scope<'tcx>>,
}

impl<'tcx> CollectorVisitor<'tcx> {
    fn new() -> Self {
        Self {
            scope_map: HashMap::new(),
        }
    }

    /// Declare a symbol from a named field in the AST node
    #[allow(dead_code)]
    fn declare_symbol(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &CollectorScopes<'tcx>,
        kind: SymKind,
        field_id: u16,
    ) -> Option<&'tcx Symbol> {
        let ident = node
            .ident_by_field(unit, field_id)
            .or_else(|| node.as_scope().and_then(|sn| sn.opt_ident()))?;

        tracing::trace!("declaring symbol '{}' of kind {:?}", ident.name, kind);
        let sym = scopes.lookup_or_insert(ident.name, node, kind)?;
        ident.set_symbol(sym);

        if let Some(sn) = node.as_scope() {
            sn.set_ident(ident);
        }

        Some(sym)
    }

    fn alloc_scope(&mut self, unit: &CompileUnit<'tcx>, symbol: &'tcx Symbol) -> &'tcx Scope<'tcx> {
        let scope = unit.cc.alloc_scope(symbol.owner());
        scope.set_symbol(symbol);
        self.scope_map.insert(scope.id(), scope);
        scope
    }

    fn get_scope(&self, scope_id: ScopeId) -> Option<&'tcx Scope<'tcx>> {
        self.scope_map.get(&scope_id).copied()
    }

    /// Lookup a symbol by name, trying primary kind first, then UnresolvedType, then inserting new
    #[allow(dead_code)]
    fn lookup_or_convert(
        &mut self,
        unit: &CompileUnit<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        name: &str,
        node: &HirNode<'tcx>,
        kind: SymKind,
    ) -> Option<&'tcx Symbol> {
        tracing::trace!(
            "looking up or converting symbol '{}' of kind {:?}",
            name,
            kind
        );

        if let Some(symbol) = scopes.lookup_symbol(name, SymKindSet::from_kind(kind)) {
            return Some(symbol);
        }

        if let Some(symbol) =
            scopes.lookup_symbol(name, SymKindSet::from_kind(SymKind::UnresolvedType))
        {
            symbol.set_kind(kind);
            return Some(symbol);
        }

        if let Some(symbol) = scopes.lookup_or_insert(name, node, kind) {
            if symbol.opt_scope().is_none() {
                let scope = self.alloc_scope(unit, symbol);
                symbol.set_scope(scope.id());
            }
            return Some(symbol);
        }

        None
    }

    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(skip_all)]
    fn visit_with_scope(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        sym: &'tcx Symbol,
        sn: &'tcx HirScope<'tcx>,
        ident: &'tcx HirIdent<'tcx>,
    ) {
        ident.set_symbol(sym);
        sn.set_ident(ident);

        let depth = scopes.scope_depth();
        if let Some(scope_id) = sym.opt_scope()
            && let Some(scope) = self.get_scope(scope_id)
        {
            sn.set_scope(scope);
            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, Some(sym));
            scopes.pop_until(depth);
            return;
        }

        let scope = self.alloc_scope(unit, sym);
        sym.set_scope(scope.id());
        sn.set_scope(scope);
        scopes.push_scope(scope);
        self.visit_children(unit, node, scopes, scope, Some(sym));
        scopes.pop_until(depth);
    }

    /// Extract the name from a declarator (handles nested declarators like pointers, references)
    fn get_declarator_name<'a>(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &'a HirNode<'tcx>,
    ) -> Option<&'tcx HirIdent<'tcx>> {
        // Check declarator field first (for nested declarators like function_declarator).
        // This is important because function_declarator's parameter_list may contain
        // identifiers (like `std` from `std::string_view`) that find_ident would pick up
        // instead of the actual function name.
        if let Some(decl) = node.child_by_field(unit, LangCpp::field_declarator) {
            return self.get_declarator_name(unit, &decl);
        }

        // Check the name field (for template_function, qualified_identifier, etc.)
        // This handles cases like `std::shared_ptr<...> getQuota()` where the declarator
        // is a qualified_identifier with nested name fields.
        if let Some(name) = node.child_by_field(unit, LangCpp::field_name) {
            if let Some(ident) = name.as_ident() {
                return Some(ident);
            }
            // Recursively check the name field for nested qualified_identifiers
            return self.get_declarator_name(unit, &name);
        }

        // Try direct identifier last
        if let Some(ident) = node.find_ident(unit) {
            return Some(ident);
        }

        None
    }

    /// Get the type name identifier from a type node.
    /// For qualified types like `std::hash<...>`, follows the `name` field
    /// to get the actual type name (e.g., `hash`) instead of the scope (e.g., `std`).
    fn get_type_name<'a>(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &'a HirNode<'tcx>,
    ) -> Option<&'tcx HirIdent<'tcx>> {
        // If node itself is an identifier, return it
        if let Some(ident) = node.as_ident() {
            return Some(ident);
        }

        // Check the name field for qualified/scoped types
        if let Some(name) = node.child_by_field(unit, LangCpp::field_name) {
            if let Some(ident) = name.as_ident() {
                return Some(ident);
            }
            // Recursively follow the name field for nested qualifications
            return self.get_type_name(unit, &name);
        }

        // Fall back to find_ident for simple cases
        node.find_ident(unit)
    }

    /// Check if a top-level declarator represents a function declaration.
    ///
    /// This distinguishes between:
    /// - `void foo();` → function declaration (returns true)
    /// - `void (*foo)();` → variable declaration of function pointer type (returns false)
    /// - `void (*signal(int sig))(int);` → function declaration returning function pointer (returns true)
    ///
    /// The rule is: A declaration is a function declaration if, from the outermost function_declarator,
    /// we can reach an identifier (the function name) by going through ONLY parenthesized_declarator nodes,
    /// OR if there's another function_declarator nested inside (which represents the actual function).
    fn is_function_declaration(&self, unit: &CompileUnit<'tcx>, decl_node: &HirNode<'tcx>) -> bool {
        // Handle init_declarator (e.g., `int (*op)(int) = foo;`)
        let decl_node = if decl_node.kind_id() == LangCpp::init_declarator {
            match decl_node.child_by_field(unit, LangCpp::field_declarator) {
                Some(inner) => inner,
                None => return false,
            }
        } else {
            *decl_node
        };

        // If top-level is not a function_declarator, it's definitely not a function declaration
        if decl_node.kind_id() != LangCpp::function_declarator {
            return false;
        }

        // Check the parameter list - if it contains only identifiers (not proper parameter_declarations),
        // this is likely a constructor call (most vexing parse), not a function declaration.
        // e.g., `Foo t(x);` where x is a variable → constructor call
        // vs `Foo t(int x);` where int is a type → function declaration
        if let Some(params) = decl_node.child_by_field(unit, LangCpp::field_parameters)
            && self.has_only_identifier_params(&params, unit)
        {
            return false;
        }

        // Get the inner declarator of the function_declarator
        let Some(inner) = decl_node.child_by_field(unit, LangCpp::field_declarator) else {
            return false;
        };

        // Check if the inner declarator leads to an identifier directly (through only parentheses),
        // OR contains another function_declarator (for functions returning function pointers).
        // If we hit a pointer_declarator before finding an identifier or another function_declarator,
        // then this is a function pointer variable, not a function declaration.
        self.is_direct_function_declarator(&inner, unit)
    }

    /// Check if a parameter_list contains only bare identifiers (no typed parameters).
    /// This indicates a constructor call in "most vexing parse" situations.
    /// e.g., `Foo t(x)` has parameter_list with just identifier `x`
    /// vs `Foo t(int x)` has parameter_list with parameter_declaration containing type `int`
    fn has_only_identifier_params(&self, params: &HirNode<'tcx>, unit: &CompileUnit<'tcx>) -> bool {
        let children = params.children(unit);

        // Filter to only semantic nodes (skip syntax tokens like parentheses and commas)
        // Syntax tokens typically have very low kind_ids (< 20) in tree-sitter
        let semantic_children: Vec<_> = children
            .iter()
            .filter(|c| {
                let kind = c.kind_id();
                // Skip common syntax tokens: ( ) , ; etc.
                // These have low kind IDs in the tree-sitter grammar
                kind > 20
            })
            .collect();

        if semantic_children.is_empty() {
            // Empty parameter list is fine for function declarations: `void foo();`
            return false;
        }

        // Check each child - if we find any proper parameter_declaration, this is a function declaration
        for child in semantic_children {
            let kind = child.kind_id();
            // These are proper parameter declarations
            if kind == LangCpp::parameter_declaration
                || kind == LangCpp::optional_parameter_declaration
                || kind == LangCpp::variadic_parameter_declaration
                || kind == LangCpp::explicit_object_parameter_declaration
            {
                return false;
            }
        }

        // All semantic children are bare identifiers (or other non-parameter-declaration nodes)
        // This looks like a constructor call, not a function declaration
        true
    }

    /// Helper: Check if this declarator is directly an identifier (possibly through parentheses),
    /// or contains another function_declarator (for functions returning function pointers).
    /// Returns false if we encounter pointer_declarator WITHOUT a nested function_declarator.
    fn is_direct_function_declarator(
        &self,
        node: &HirNode<'tcx>,
        unit: &CompileUnit<'tcx>,
    ) -> bool {
        let kind_id = node.kind_id();

        // If this is an identifier, we found the function name - this IS a function declaration
        if kind_id == LangCpp::identifier {
            return true;
        }

        // If this is another function_declarator, we found a nested function
        // (e.g., signal in `void (*signal(int))(int)`) - this IS a function declaration
        if kind_id == LangCpp::function_declarator {
            return true;
        }

        // If we hit pointer_declarator or reference_declarator, check if there's a nested
        // function_declarator inside. If so, this IS a function that returns a pointer/reference.
        // If not, this is a function pointer/reference variable declaration.
        if kind_id == LangCpp::pointer_declarator || kind_id == LangCpp::reference_declarator {
            // Check if there's a function_declarator nested inside
            if let Some(inner) = node.child_by_field(unit, LangCpp::field_declarator) {
                return self.contains_function_declarator(&inner, unit);
            }
            return false;
        }

        // If we hit array_declarator, the outer function_declarator describes a type, not a function
        if kind_id == LangCpp::array_declarator {
            return false;
        }

        // Continue through parenthesized_declarator
        if kind_id == LangCpp::parenthesized_declarator {
            if let Some(inner) = node.child_by_field(unit, LangCpp::field_declarator) {
                return self.is_direct_function_declarator(&inner, unit);
            }
            // Check first child if no declarator field
            for &child_id in node.child_ids() {
                let child = unit.hir_node(child_id);
                // Skip parentheses tokens
                if child.kind_id() != 5 && child.kind_id() != 8 {
                    // ( and )
                    return self.is_direct_function_declarator(&child, unit);
                }
            }
        }

        // For other node types, check the nested declarator
        if let Some(inner) = node.child_by_field(unit, LangCpp::field_declarator) {
            return self.is_direct_function_declarator(&inner, unit);
        }

        false
    }

    /// Check if this declarator tree contains a function_declarator anywhere inside
    fn contains_function_declarator(&self, node: &HirNode<'tcx>, unit: &CompileUnit<'tcx>) -> bool {
        if node.kind_id() == LangCpp::function_declarator {
            return true;
        }

        if let Some(inner) = node.child_by_field(unit, LangCpp::field_declarator) {
            return self.contains_function_declarator(&inner, unit);
        }

        // Check children for parenthesized_declarator without field
        for &child_id in node.child_ids() {
            let child = unit.hir_node(child_id);
            if child.kind_id() != 5 && child.kind_id() != 8 {
                // Skip ( and )
                if self.contains_function_declarator(&child, unit) {
                    return true;
                }
            }
        }

        false
    }

    /// Find the innermost function_declarator that contains the actual function name.
    /// For `void (*signal(int))(int)`, this returns the inner `signal(int)` function_declarator.
    /// For `void foo()`, this returns the `foo()` function_declarator.
    fn find_innermost_function_declarator(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
    ) -> Option<HirNode<'tcx>> {
        // Check if this node is a function_declarator
        if node.kind_id() == LangCpp::function_declarator {
            // Look for a nested function_declarator inside
            if let Some(inner) = node.child_by_field(unit, LangCpp::field_declarator)
                && let Some(nested) = self.find_nested_function_declarator(&inner, unit)
            {
                return Some(nested);
            }
            // No nested function_declarator, this is the innermost
            return Some(*node);
        }

        // Look in the nested declarator
        if let Some(inner) = node.child_by_field(unit, LangCpp::field_declarator) {
            return self.find_innermost_function_declarator(unit, &inner);
        }

        None
    }

    /// Helper to find a function_declarator nested inside pointer/parenthesized declarators
    fn find_nested_function_declarator(
        &self,
        node: &HirNode<'tcx>,
        unit: &CompileUnit<'tcx>,
    ) -> Option<HirNode<'tcx>> {
        if node.kind_id() == LangCpp::function_declarator {
            // Found a nested function_declarator, check if there's an even deeper one
            if let Some(inner) = node.child_by_field(unit, LangCpp::field_declarator)
                && let Some(deeper) = self.find_nested_function_declarator(&inner, unit)
            {
                return Some(deeper);
            }
            return Some(*node);
        }

        // For parenthesized_declarator, the inner declarator is a direct child, not a field
        if node.kind_id() == LangCpp::parenthesized_declarator {
            let children = node.children(unit);
            for child in children {
                // Skip punctuation
                if child.kind_id() == 5 || child.kind_id() == 8 {
                    // ( or )
                    continue;
                }
                if let Some(found) = self.find_nested_function_declarator(&child, unit) {
                    return Some(found);
                }
            }
            return None;
        }

        // Continue through pointer declarators using the declarator field
        if let Some(inner) = node.child_by_field(unit, LangCpp::field_declarator) {
            return self.find_nested_function_declarator(&inner, unit);
        }

        None
    }

    /// Create a synthetic identifier for operator names
    fn get_operator_name<'a>(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &'a HirNode<'tcx>,
    ) -> Option<&'tcx HirIdent<'tcx>> {
        use llmcc_core::ir::{HirBase, HirId, HirIdent, HirKind};
        use smallvec::SmallVec;

        // For operator overloads, look for operator_name node
        for &child_id in node.child_ids() {
            let child = unit.hir_node(child_id);
            if child.kind_id() == LangCpp::operator_name {
                // Create a synthetic ident for the operator
                let text = unit.hir_text(&child);
                // Allocate the string in the arena to get a &'tcx str
                let name: &'tcx str = unit.cc.arena().alloc_str(&text);

                // Create a HirBase for the synthetic identifier
                let base = HirBase {
                    id: HirId(child_id.0),
                    parent: None,
                    kind_id: LangCpp::identifier,
                    start_byte: child.start_byte(),
                    end_byte: child.end_byte(),
                    kind: HirKind::Identifier,
                    field_id: u16::MAX,
                    children: SmallVec::new(),
                };
                let ident = unit.cc.arena().alloc(HirIdent::new(base, name));
                return Some(ident);
            }
            // Recursively search in nested declarators
            if let Some(ident) = self.get_operator_name(unit, &child) {
                return Some(ident);
            }
        }
        None
    }
}

impl<'tcx> AstVisitorCpp<'tcx, CollectorScopes<'tcx>> for CollectorVisitor<'tcx> {
    // Root: translation_unit
    fn visit_translation_unit(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let file_path = unit.file_path().unwrap();
        tracing::trace!("collecting translation_unit: {}", file_path);

        let depth = scopes.scope_depth();
        let sn = node.as_scope();
        let meta = unit.unit_meta();

        // Track package scope for parent relationships
        let mut package_scope: Option<&'tcx Scope<'tcx>> = None;

        // Set up package (project) scope from unit metadata
        if let Some(ref package_name) = meta.package_name
            && let Some(symbol) = scopes.lookup_or_insert_global(package_name, node, SymKind::Crate)
        {
            tracing::trace!("insert package symbol in globals '{}'", package_name);
            scopes.push_scope_with(node, Some(symbol));
            package_scope = scopes.top();
        }

        // For files in subdirectories, create a module scope for proper hierarchy traversal
        let mut module_wrapper_scope: Option<&'tcx Scope<'tcx>> = None;
        if let Some(ref module_name) = meta.module_name
            && let Some(module_sym) =
                scopes.lookup_or_insert_global(module_name, node, SymKind::Module)
        {
            tracing::trace!("insert module symbol in globals '{}'", module_name);
            let mod_scope = self.alloc_scope(unit, module_sym);
            if let Some(pkg_s) = package_scope {
                mod_scope.add_parent(pkg_s);
            }
            module_wrapper_scope = Some(mod_scope);
        }

        // Set up file scope
        if let Some(ref file_name) = meta.file_name
            && let Some(file_sym) = scopes.lookup_or_insert_global(file_name, node, SymKind::File)
        {
            file_sym.set_is_global(true);

            // Allocate file ident to set the proper name on the root block
            let arena_name = unit.cc.arena().alloc_str(file_name);
            let ident = unit
                .cc
                .alloc_file_ident(next_hir_id(), arena_name, file_sym);
            ident.set_symbol(file_sym);

            let file_scope = self.alloc_scope(unit, file_sym);

            // Connect to module or package scope
            if let Some(mod_scope) = module_wrapper_scope {
                file_scope.add_parent(mod_scope);
                mod_scope.add_parent(package_scope.unwrap_or(namespace));
            } else if let Some(pkg_scope) = package_scope {
                file_scope.add_parent(pkg_scope);
            }

            if let Some(sn) = sn {
                sn.set_ident(ident);
                sn.set_scope(file_scope);
            }
            scopes.push_scope(file_scope);

            self.visit_children(unit, node, scopes, file_scope, Some(file_sym));
            scopes.pop_until(depth);
            return;
        }

        self.visit_children(unit, node, scopes, namespace, None);
    }

    // Namespace definition
    fn visit_namespace_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let Some(sn) = node.as_scope() else { return };

        // Get namespace name from the 'name' field
        let Some(name_ident) = node.ident_by_field(unit, LangCpp::field_name) else {
            // Anonymous namespace - still need to set up scope
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);
            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, None);
            scopes.pop_scope();
            return;
        };

        let sym = match scopes.lookup_or_insert(name_ident.name, node, SymKind::Module) {
            Some(s) => s,
            None => {
                // Still need to set up scope even if symbol creation failed
                let scope = unit.cc.alloc_scope(node.id());
                sn.set_scope(scope);
                scopes.push_scope(scope);
                self.visit_children(unit, node, scopes, scope, None);
                scopes.pop_scope();
                return;
            }
        };

        self.visit_with_scope(unit, node, scopes, sym, sn, name_ident);
    }

    // Class specifier
    fn visit_class_specifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let Some(sn) = node.as_scope() else { return };

        // Get the name field - for qualified types like `struct std::hash<...>`,
        // we need to follow the name field recursively to get the actual type name.
        let name_node = node.child_by_field(unit, LangCpp::field_name);
        let name_ident = name_node.and_then(|n| self.get_type_name(unit, &n));

        let Some(name_ident) = name_ident else {
            // Anonymous class/struct - still need to set up scope for nested members
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);
            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, None);
            scopes.pop_scope();
            return;
        };

        let sym = match scopes.lookup_or_insert(name_ident.name, node, SymKind::Struct) {
            Some(s) => s,
            None => {
                // Still need to set up scope even if symbol creation failed
                let scope = unit.cc.alloc_scope(node.id());
                sn.set_scope(scope);
                scopes.push_scope(scope);
                self.visit_children(unit, node, scopes, scope, None);
                scopes.pop_scope();
                return;
            }
        };

        self.visit_with_scope(unit, node, scopes, sym, sn, name_ident);

        // Add struct to globals for cross-file type resolution (like Rust does)
        sym.set_is_global(true);
        scopes.globals().insert(sym);
    }

    // Struct specifier
    fn visit_struct_specifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Struct is essentially the same as class in C++
        self.visit_class_specifier(unit, node, scopes, namespace, parent);
    }

    // Union specifier
    fn visit_union_specifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Union is similar to struct
        self.visit_class_specifier(unit, node, scopes, namespace, parent);
    }

    // Enum specifier
    fn visit_enum_specifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let Some(sn) = node.as_scope() else { return };

        let Some(name_ident) = node.ident_by_field(unit, LangCpp::field_name) else {
            // Anonymous enum - still need to set up scope for nested enumerators
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);
            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, None);
            scopes.pop_scope();
            return;
        };

        let sym = match scopes.lookup_or_insert(name_ident.name, node, SymKind::Enum) {
            Some(s) => s,
            None => {
                // Still need to set up scope even if symbol creation failed
                let scope = unit.cc.alloc_scope(node.id());
                sn.set_scope(scope);
                scopes.push_scope(scope);
                self.visit_children(unit, node, scopes, scope, None);
                scopes.pop_scope();
                return;
            }
        };

        self.visit_with_scope(unit, node, scopes, sym, sn, name_ident);

        // Add enum to globals for cross-file type resolution
        sym.set_is_global(true);
        scopes.globals().insert(sym);
    }

    // Enumerator
    fn visit_enumerator(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(name_ident) = node.ident_by_field(unit, LangCpp::field_name)
            && let Some(sym) = scopes.lookup_or_insert(name_ident.name, node, SymKind::Const)
        {
            name_ident.set_symbol(sym);

            // Set field_of to parent enum
            if let Some(p) = parent {
                sym.set_field_of(p.id());
            }
        }

        self.visit_children(unit, node, scopes, namespace, parent);
    }

    // Function definition
    fn visit_function_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let Some(sn) = node.as_scope() else { return };

        // Get declarator which contains the function name
        let Some(decl_node) = node.child_by_field(unit, LangCpp::field_declarator) else {
            // No declarator, still need to visit children to set up nested scopes
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);
            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
            return;
        };

        // Try regular identifier first, then operator name
        let name_ident = self
            .get_declarator_name(unit, &decl_node)
            .or_else(|| self.get_operator_name(unit, &decl_node));

        let Some(name_ident) = name_ident else {
            // Still visit children to set up nested scopes even without a name
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);
            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
            return;
        };

        // Determine if this is a method or a free function
        let kind = if is_method_context(parent) {
            SymKind::Method
        } else {
            SymKind::Function
        };

        let sym = match scopes.lookup_or_insert(name_ident.name, node, kind) {
            Some(s) => s,
            None => {
                // Still need to set up scope even if symbol creation failed
                let scope = unit.cc.alloc_scope(node.id());
                sn.set_scope(scope);
                scopes.push_scope(scope);
                self.visit_children(unit, node, scopes, scope, parent);
                scopes.pop_scope();
                return;
            }
        };

        self.visit_with_scope(unit, node, scopes, sym, sn, name_ident);

        // Free functions are global (like Rust does)
        if kind == SymKind::Function {
            sym.set_is_global(true);
            scopes.globals().insert(sym);
        }
    }

    // Template declaration
    fn visit_template_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Process template parameters first, then visit children
        // The actual class/function/etc. declaration is a child
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    // Field declaration
    fn visit_field_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Get declarator which contains the field name
        if let Some(decl_node) = node.child_by_field(unit, LangCpp::field_declarator)
            && let Some(name_ident) = self.get_declarator_name(unit, &decl_node)
            && let Some(sym) = scopes.lookup_or_insert(name_ident.name, node, SymKind::Field)
        {
            name_ident.set_symbol(sym);

            // Set field_of to parent struct/class
            if let Some(p) = parent {
                sym.set_field_of(p.id());
            }
        }

        self.visit_children(unit, node, scopes, namespace, parent);
    }

    // Parameter declaration
    fn visit_parameter_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Get declarator which contains the parameter name
        if let Some(decl_node) = node.child_by_field(unit, LangCpp::field_declarator)
            && let Some(name_ident) = self.get_declarator_name(unit, &decl_node)
            && let Some(sym) = scopes.lookup_or_insert(name_ident.name, node, SymKind::Variable)
        {
            name_ident.set_symbol(sym);
        }

        self.visit_children(unit, node, scopes, namespace, parent);
    }

    // Compound statement (block)
    fn visit_compound_statement(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);

            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    // Declaration (variable declarations, function declarations/prototypes)
    fn visit_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Check if we're inside a function body.
        // Inside a function body, declarations with function_declarator syntax are always
        // variable declarations (constructor calls), not function declarations.
        // You cannot forward-declare functions inside a function in C++.
        let in_function_body =
            parent.is_some_and(|p| matches!(p.kind(), SymKind::Function | SymKind::Method));

        // Get declarator which contains the name
        if let Some(decl_node) = node.child_by_field(unit, LangCpp::field_declarator) {
            // Check if this is a function declaration (forward declaration/prototype)
            // e.g., "void normal_func();" - but NOT "void (*func_ptr)();" which is a variable
            // IMPORTANT: Inside function bodies, we never have function declarations,
            // so treat them as variable declarations (constructor calls).
            if !in_function_body && self.is_function_declaration(unit, &decl_node) {
                // This is a function declaration (prototype)
                // Find the innermost function_declarator which contains the actual function name
                if let Some(func_decl) = self.find_innermost_function_declarator(unit, &decl_node)
                    && let Some(name_ident) = self.get_declarator_name(unit, &func_decl)
                {
                    // Determine if this is a method or free function
                    let kind = if is_method_context(parent) {
                        SymKind::Method
                    } else {
                        SymKind::Function
                    };

                    if let Some(sym) = scopes.lookup_or_insert(name_ident.name, &func_decl, kind) {
                        // Set up the scope for this function declaration
                        // The HirScope node needs to have its scope set for graph_builder to create a block
                        if let Some(scope_node) = func_decl.as_scope() {
                            let scope = self.alloc_scope(unit, sym);
                            sym.set_scope(scope.id());
                            scope_node.set_scope(scope);
                            name_ident.set_symbol(sym);

                            scopes.push_scope(scope);
                            self.visit_children(unit, &func_decl, scopes, scope, Some(sym));
                            scopes.pop_scope();

                            // Free functions are global
                            if kind == SymKind::Function {
                                sym.set_is_global(true);
                                scopes.globals().insert(sym);
                            }
                            return;
                        }
                    }
                }
                // Fallthrough to visit_children if we couldn't create a proper symbol
            } else {
                // This is a variable declaration (possibly with constructor-style init like `Foo t(x)`)
                if let Some(name_ident) = self.get_declarator_name(unit, &decl_node)
                    && let Some(sym) =
                        scopes.lookup_or_insert(name_ident.name, node, SymKind::Variable)
                {
                    name_ident.set_symbol(sym);

                    // Top-level declarations are global
                    if scopes.scope_depth() <= 2 {
                        sym.set_is_global(true);
                        scopes.globals().insert(sym);
                    }
                }
                // For variable declarations with function_declarator syntax (constructor-style init),
                // don't visit children - the "parameters" in `Foo t(x)` are actually constructor arguments.
                // But DO visit children for other variable declarations like `auto add = [](int a, int b) { ... }`
                // which may contain lambdas that should be processed.
                if decl_node.kind_id() == LangCpp::function_declarator {
                    return;
                }
            }
        }

        self.visit_children(unit, node, scopes, namespace, parent);
    }

    // Type definition (typedef)
    fn visit_type_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let Some(sn) = node.as_scope() else {
            return;
        };

        // Get declarator which contains the type alias name
        if let Some(decl_node) = node.child_by_field(unit, LangCpp::field_declarator)
            && let Some(name_ident) = self.get_declarator_name(unit, &decl_node)
            && let Some(sym) = scopes.lookup_or_insert(name_ident.name, node, SymKind::TypeAlias)
        {
            name_ident.set_symbol(sym);
            sn.set_ident(name_ident);

            if scopes.scope_depth() <= 2 {
                sym.set_is_global(true);
                scopes.globals().insert(sym);
            }
        }
    }

    // Alias declaration (using = ...)
    fn visit_alias_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let Some(sn) = node.as_scope() else {
            return;
        };

        if let Some(name_ident) = node.ident_by_field(unit, LangCpp::field_name)
            && let Some(sym) = scopes.lookup_or_insert(name_ident.name, node, SymKind::TypeAlias)
        {
            name_ident.set_symbol(sym);
            sn.set_ident(name_ident);

            if scopes.scope_depth() <= 2 {
                sym.set_is_global(true);
                scopes.globals().insert(sym);
            }
        }
    }

    // Concept definition (C++20)
    fn visit_concept_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let Some(sn) = node.as_scope() else {
            return;
        };

        if let Some(name_ident) = node.ident_by_field(unit, LangCpp::field_name)
            && let Some(sym) = scopes.lookup_or_insert(name_ident.name, node, SymKind::Trait)
        {
            self.visit_with_scope(unit, node, scopes, sym, sn, name_ident);
        }
    }

    // Lambda expression
    fn visit_lambda_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);

            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    // Function declarator (nested within function_definition, creates its own scope for parameters)
    fn visit_function_declarator(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);

            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    // Template function
    fn visit_template_function(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);

            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    // Template method
    fn visit_template_method(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);

            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    // Type parameter declaration (template<typename T>)
    fn visit_type_parameter_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);

            // Register the type parameter as a symbol
            if let Some(ident) = node.find_ident(unit)
                && let Some(sym) = scopes.lookup_or_insert(ident.name, node, SymKind::TypeParameter)
            {
                ident.set_symbol(sym);
            }

            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    // Variadic type parameter declaration (template<typename... Args>)
    fn visit_variadic_type_parameter_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);

            // Register the type parameter as a symbol
            if let Some(ident) = node.find_ident(unit)
                && let Some(sym) = scopes.lookup_or_insert(ident.name, node, SymKind::TypeParameter)
            {
                ident.set_symbol(sym);
            }

            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    // Optional type parameter declaration (C++ default template types)
    fn visit_optional_type_parameter_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);

            // Register the type parameter as a symbol
            if let Some(ident) = node.find_ident(unit)
                && let Some(sym) = scopes.lookup_or_insert(ident.name, node, SymKind::TypeParameter)
            {
                ident.set_symbol(sym);
            }

            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    // Template template parameter declaration
    fn visit_template_template_parameter_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);

            // Register the template template parameter as a symbol
            if let Some(ident) = node.find_ident(unit)
                && let Some(sym) = scopes.lookup_or_insert(ident.name, node, SymKind::TypeParameter)
            {
                ident.set_symbol(sym);
            }

            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    // Explicit object parameter declaration (C++23 deducing this)
    fn visit_explicit_object_parameter_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);

            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    // Field declaration list (class body)
    fn visit_field_declaration_list(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);

            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    // Catch clause
    fn visit_catch_clause(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);

            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    // Using declaration
    fn visit_using_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);

            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    // Requires expression (C++20)
    fn visit_requires_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);

            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    // Module declaration (C++20)
    fn visit_module_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let Some(sn) = node.as_scope() else { return };

        if let Some(name_ident) = node.find_ident(unit)
            && let Some(sym) = scopes.lookup_or_insert(name_ident.name, node, SymKind::Module)
        {
            self.visit_with_scope(unit, node, scopes, sym, sn, name_ident);
        }
    }
}

/// Entry point for symbol collection
pub fn collect_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    scope_stack: ScopeStack<'tcx>,
    _config: &ResolverOption,
) -> &'tcx Scope<'tcx> {
    use llmcc_core::ir::HirId;

    let cc = unit.cc;
    let arena = cc.arena();
    let unit_globals_val = Scope::new(HirId(unit.index));
    let scope_id = unit_globals_val.id().0;
    let unit_globals = arena.alloc_with_id(scope_id, unit_globals_val);
    let mut scopes = CollectorScopes::new(cc, unit.index, scope_stack, unit_globals);

    let mut visitor = CollectorVisitor::new();
    visitor.visit_node(&unit, node, &mut scopes, unit_globals, None);

    unit_globals
}
