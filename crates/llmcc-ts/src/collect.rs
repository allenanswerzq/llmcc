use std::collections::HashMap;

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirId, HirIdent, HirNode, HirScope};
use llmcc_core::next_hir_id;
use llmcc_core::scope::{Scope, ScopeStack};
use llmcc_core::symbol::{ScopeId, SymKind, SymKindSet, Symbol};
use llmcc_resolver::{CollectorScopes, ResolverOption};

use crate::LangTypeScript;
use crate::token::AstVisitorTypeScript;

/// Check if a node is exported at the top level (parent is export_statement at file level)
/// Exports inside namespaces are not considered global exports.
fn is_exported(unit: &CompileUnit, node: &HirNode) -> bool {
    if let Some(parent_id) = node.parent()
        && let Some(parent_node) = unit.opt_hir_node(parent_id)
        && parent_node.kind_id() == LangTypeScript::export_statement
        && let Some(grandparent_id) = parent_node.parent()
        && let Some(grandparent_node) = unit.opt_hir_node(grandparent_id)
    {
        // If grandparent is program or statement_block at file level, it's a top-level export
        let gp_kind = grandparent_node.kind_id();
        return gp_kind == LangTypeScript::program || gp_kind == LangTypeScript::export_statement;
    }
    false
}

/// Check if a function is in a method context (parent is a class or interface)
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
}

impl<'tcx> AstVisitorTypeScript<'tcx, CollectorScopes<'tcx>> for CollectorVisitor<'tcx> {
    // Root: program node
    fn visit_program(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let file_path = unit.file_path().unwrap();
        let _ = file_path; // Used for debugging

        let depth = scopes.scope_depth();
        let sn = node.as_scope();
        let meta = unit.unit_meta();

        // Track package scope for parent relationships
        let mut package_scope: Option<&'tcx Scope<'tcx>> = None;

        // Set up package (crate) scope from unit metadata
        if let Some(ref package_name) = meta.package_name
            && let Some(symbol) = scopes.lookup_or_insert_global(package_name, node, SymKind::Crate)
        {
            scopes.push_scope_with(node, Some(symbol));
            package_scope = scopes.top();
        }

        // For files in subdirectories, create a module scope for proper hierarchy traversal
        let mut module_wrapper_scope: Option<&'tcx Scope<'tcx>> = None;
        if let Some(ref module_name) = meta.module_name
            && let Some(module_sym) =
                scopes.lookup_or_insert_global(module_name, node, SymKind::Module)
        {
            let mod_scope = self.alloc_scope(unit, module_sym);
            if let Some(pkg_s) = package_scope {
                mod_scope.add_parent(pkg_s);
            }
            module_wrapper_scope = Some(mod_scope);
        }

        // Create file symbol and scope
        if let Some(ref file_name) = meta.file_name {
            let file_sym = scopes.lookup_or_insert(file_name, node, SymKind::File);
            if let Some(file_sym) = file_sym {
                let arena_name = unit.cc.arena().alloc_str(file_name);
                let ident = unit
                    .cc
                    .alloc_file_ident(next_hir_id(), arena_name, file_sym);
                ident.set_symbol(file_sym);

                let file_scope = self.alloc_scope(unit, file_sym);
                file_sym.set_scope(file_scope.id());

                // Set up parent relationships for hierarchy traversal
                // Add package scope first (if any)
                if let Some(pkg_scope) = package_scope {
                    file_scope.add_parent(pkg_scope);
                }
                // Add module scope (if any)
                if let Some(mod_scope) = module_wrapper_scope {
                    file_scope.add_parent(mod_scope);
                }

                // Set the scope on the HirScope node
                if let Some(sn) = sn {
                    sn.set_ident(ident);
                    sn.set_scope(file_scope);
                }

                scopes.push_scope(file_scope);
                self.visit_children(unit, node, scopes, file_scope, Some(file_sym));
                scopes.pop_until(depth);
                return;
            }
        }

        self.visit_children(unit, node, scopes, namespace, None);
    }

    // Class declaration
    fn visit_class_declaration(
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

        if let Some(ident) = node.ident_by_field(unit, LangTypeScript::field_name) {
            // Use global scope for exported classes to enable cross-file resolution
            let sym = if is_exported(unit, node) {
                scopes.lookup_or_insert_global(ident.name, node, SymKind::Struct)
            } else {
                scopes.lookup_or_insert(ident.name, node, SymKind::Struct)
            };
            if let Some(sym) = sym {
                self.visit_with_scope(unit, node, scopes, sym, sn, ident);
            }
        }
    }

    // Abstract class declaration
    fn visit_abstract_class_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_class_declaration(unit, node, scopes, namespace, parent);
    }

    // Internal module (namespace)
    fn visit_internal_module(
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

        if let Some(ident) = node.ident_by_field(unit, LangTypeScript::field_name) {
            let sym = scopes.lookup_or_insert(ident.name, node, SymKind::Namespace);
            if let Some(sym) = sym {
                self.visit_with_scope(unit, node, scopes, sym, sn, ident);
            }
        }
    }

    // Interface declaration
    fn visit_interface_declaration(
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

        if let Some(ident) = node.ident_by_field(unit, LangTypeScript::field_name) {
            // Use global scope for exported interfaces to enable cross-file resolution
            let sym = if is_exported(unit, node) {
                scopes.lookup_or_insert_global(ident.name, node, SymKind::Interface)
            } else {
                scopes.lookup_or_insert(ident.name, node, SymKind::Interface)
            };
            if let Some(sym) = sym {
                self.visit_with_scope(unit, node, scopes, sym, sn, ident);
            }
        }
    }

    // Type alias declaration
    fn visit_type_alias_declaration(
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

        if let Some(ident) = node.ident_by_field(unit, LangTypeScript::field_name) {
            // Use global scope for exported type aliases to enable cross-file resolution
            let sym = if is_exported(unit, node) {
                scopes.lookup_or_insert_global(ident.name, node, SymKind::TypeAlias)
            } else {
                scopes.lookup_or_insert(ident.name, node, SymKind::TypeAlias)
            };
            if let Some(sym) = sym {
                self.visit_with_scope(unit, node, scopes, sym, sn, ident);
            }
        }
    }

    // Enum declaration
    fn visit_enum_declaration(
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

        if let Some(ident) = node.ident_by_field(unit, LangTypeScript::field_name) {
            // Use global scope for exported enums to enable cross-file resolution
            let sym = if is_exported(unit, node) {
                scopes.lookup_or_insert_global(ident.name, node, SymKind::Enum)
            } else {
                scopes.lookup_or_insert(ident.name, node, SymKind::Enum)
            };
            if let Some(sym) = sym {
                self.visit_with_scope(unit, node, scopes, sym, sn, ident);
            }
        }
    }

    // Function declaration
    fn visit_function_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let Some(sn) = node.as_scope() else {
            return;
        };

        if let Some(ident) = node.ident_by_field(unit, LangTypeScript::field_name) {
            let kind = if is_method_context(parent) {
                SymKind::Method
            } else {
                SymKind::Function
            };
            // Use global scope for exported functions to enable cross-file resolution
            let sym = if is_exported(unit, node) && kind == SymKind::Function {
                scopes.lookup_or_insert_global(ident.name, node, kind)
            } else {
                scopes.lookup_or_insert(ident.name, node, kind)
            };
            if let Some(sym) = sym {
                self.visit_with_scope(unit, node, scopes, sym, sn, ident);
            }
        }
    }

    // Function signature (declare function externalFn(x: number): string;)
    fn visit_function_signature(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let Some(sn) = node.as_scope() else {
            return;
        };

        if let Some(ident) = node.ident_by_field(unit, LangTypeScript::field_name) {
            let kind = if is_method_context(parent) {
                SymKind::Method
            } else {
                SymKind::Function
            };
            let sym = scopes.lookup_or_insert(ident.name, node, kind);
            if let Some(sym) = sym {
                self.visit_with_scope(unit, node, scopes, sym, sn, ident);
            }
        }
    }

    // Generator function declaration (function* name() { ... })
    fn visit_generator_function_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let Some(sn) = node.as_scope() else {
            return;
        };

        if let Some(ident) = node.ident_by_field(unit, LangTypeScript::field_name) {
            let kind = if is_method_context(parent) {
                SymKind::Method
            } else {
                SymKind::Function
            };
            let sym = scopes.lookup_or_insert(ident.name, node, kind);
            if let Some(sym) = sym {
                self.visit_with_scope(unit, node, scopes, sym, sn, ident);
            }
        }
    }

    // Method definition
    fn visit_method_definition(
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

        if let Some(ident) = node.ident_by_field(unit, LangTypeScript::field_name) {
            let sym = scopes.lookup_or_insert(ident.name, node, SymKind::Method);
            if let Some(sym) = sym {
                self.visit_with_scope(unit, node, scopes, sym, sn, ident);
            }
        }
    }

    // Arrow function
    fn visit_arrow_function(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            // Arrow functions are anonymous - create a scope and set it on the HirScope
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);
            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    // Variable declarator
    fn visit_variable_declarator(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Get the name of the variable
        if let Some(name_node) = node.child_by_field(unit, LangTypeScript::field_name)
            && let Some(ident) = name_node.as_ident()
        {
            let sym = scopes.lookup_or_insert(ident.name, node, SymKind::Variable);
            if let Some(sym) = sym {
                ident.set_symbol(sym);
            }
        }

        self.visit_children(unit, node, scopes, namespace, parent);
    }

    // Required parameter
    fn visit_required_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Parameter name binding - parameters are stored as variables
        if let Some(pattern_node) = node.child_by_field(unit, LangTypeScript::field_pattern)
            && let Some(ident) = pattern_node.as_ident()
        {
            let sym = scopes.lookup_or_insert(ident.name, node, SymKind::Variable);
            if let Some(sym) = sym {
                ident.set_symbol(sym);
            }
        }

        self.visit_children(unit, node, scopes, namespace, parent);
    }

    // Rest pattern (e.g., ...args in function(...args: T[]))
    fn visit_rest_pattern(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Rest pattern contains an identifier directly as a child
        if let Some(ident) = node.find_ident(unit) {
            let sym = scopes.lookup_or_insert(ident.name, node, SymKind::Variable);
            if let Some(sym) = sym {
                ident.set_symbol(sym);
            }
        }

        self.visit_children(unit, node, scopes, namespace, parent);
    }

    // Optional parameter
    fn visit_optional_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Same as required parameter - parameters are stored as variables
        if let Some(pattern_node) = node.child_by_field(unit, LangTypeScript::field_pattern)
            && let Some(ident) = pattern_node.as_ident()
        {
            let sym = scopes.lookup_or_insert(ident.name, node, SymKind::Variable);
            if let Some(sym) = sym {
                ident.set_symbol(sym);
            }
        }

        self.visit_children(unit, node, scopes, namespace, parent);
    }

    // Type parameter (e.g., T in function<T extends HasLength>)
    fn visit_type_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // type_parameter has a name field with the type parameter identifier (e.g., T)
        if let Some(ident) = node.ident_by_field(unit, LangTypeScript::field_name) {
            let sym = scopes.lookup_or_insert(ident.name, node, SymKind::TypeParameter);
            if let Some(sym) = sym {
                ident.set_symbol(sym);
            }
        }

        self.visit_children(unit, node, scopes, namespace, parent);
    }

    // Property signature (interface field)
    fn visit_property_signature(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(ident) = node.ident_by_field(unit, LangTypeScript::field_name) {
            let sym = scopes.lookup_or_insert(ident.name, node, SymKind::Field);
            if let Some(sym) = sym {
                ident.set_symbol(sym);
            }
        }

        self.visit_children(unit, node, scopes, namespace, parent);
    }

    // Public field definition (class field)
    fn visit_public_field_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(ident) = node.ident_by_field(unit, LangTypeScript::field_name) {
            let sym = scopes.lookup_or_insert(ident.name, node, SymKind::Field);
            if let Some(sym) = sym {
                ident.set_symbol(sym);
                // Set the ident on the scope so BlockField can get the name
                if let Some(sn) = node.as_scope() {
                    sn.set_ident(ident);
                }
            }
        }

        self.visit_children(unit, node, scopes, namespace, parent);
    }

    // Method signature (interface method)
    fn visit_method_signature(
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

        if let Some(ident) = node.ident_by_field(unit, LangTypeScript::field_name) {
            let sym = scopes.lookup_or_insert(ident.name, node, SymKind::Method);
            if let Some(sym) = sym {
                self.visit_with_scope(unit, node, scopes, sym, sn, ident);
            }
        }
    }

    // Abstract method signature
    fn visit_abstract_method_signature(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_method_signature(unit, node, scopes, namespace, parent);
    }

    // Enum member
    fn visit_enum_assignment(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Get enum variant name from the first child (property_identifier)
        if let Some(name_node) = node.child_ids().first().map(|id| unit.hir_node(*id))
            && let Some(ident) = name_node.as_ident()
        {
            let sym = scopes.lookup_or_insert(ident.name, node, SymKind::EnumVariant);
            if let Some(sym) = sym {
                ident.set_symbol(sym);
                // Also set on scope so graph_builder extracts the correct symbol
                if let Some(sn) = node.as_scope() {
                    sn.set_ident(ident);
                }
            }
        }

        self.visit_children(unit, node, scopes, namespace, parent);
    }

    // Anonymous scope handlers - these create scopes without symbols

    /// Handle statement_block - function bodies, if/for/while bodies, etc.
    fn visit_statement_block(
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

    /// Handle function_expression - anonymous functions
    fn visit_function_expression(
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

    /// Handle generator_function - generator function expressions
    fn visit_generator_function(
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

    /// Handle class - class expressions (anonymous classes)
    fn visit_class(
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

    /// Handle class_body - body of class declarations
    fn visit_class_body(
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

    /// Handle interface_body - body of interface declarations
    fn visit_interface_body(
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

    /// Handle enum_body - body of enum declarations
    fn visit_enum_body(
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

    /// Handle module - ES module wrapper
    fn visit_module(
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

    /// Handle call_signature - function call signatures in interfaces
    fn visit_call_signature(
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

    /// Handle construct_signature - constructor signatures in interfaces
    fn visit_construct_signature(
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

    /// Handle tuple_type - tuple type definitions
    fn visit_tuple_type(
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

    /// Handle object_type - object type literals
    fn visit_object_type(
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
}

pub fn collect_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    scope_stack: ScopeStack<'tcx>,
    _config: &ResolverOption,
) -> &'tcx Scope<'tcx> {
    let cc = unit.cc;
    let arena = cc.arena();
    let unit_globals_val = Scope::new(HirId(unit.index));
    let scope_id = unit_globals_val.id().0;
    let unit_globals = arena.alloc_with_id(scope_id, unit_globals_val);
    let mut scopes = CollectorScopes::new(cc, unit.index, scope_stack, unit_globals);

    let mut visit = CollectorVisitor::new();
    visit.visit_node(&unit, node, &mut scopes, unit_globals, None);

    unit_globals
}
