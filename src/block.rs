use crate::AstArena as BlockArena;
use crate::AstArenaShare as BlockArenaShare;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockKind {
    Undefined,
    Function,
    Class,
    Method,
    Variable,
    Constant,
    Import,
    Module,
    Statement,
    // Impl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockRelation {
    Unknown,
    // Function/method calls
    Calls,
    Called,
    // Class/inheritance relationships
    Inherits,
    Inherited,
    // Variable/data relationships
    Defines,
    Uses,
    Modifies,
    // Module/import relationships
    Imports,
    Exports,
    // Control flow
    Contains,
    ContainedBy,
    // Dependencies
    DependsOn,
    DependedBy,
}

#[derive(Debug, Clone)]
pub struct BlockBase {
    arena: BlockArenaShare<BasicBlock>,
    id: usize,
    // The AST node id associated with this block
    ast_id: usize,
    kind: BlockKind,
    // Relationships to other blocks
    related_blocks: HashMap<BlockRelation, Vec<usize>>,
    // Metadata
    attributes: HashMap<String, String>,
}

impl BlockBase {
    pub fn new(id: usize, ast_id: usize, kind: BlockKind) -> Self {
        Self {
            arena: BlockArena::new(),
            id,
            ast_id,
            kind,
            related_blocks: HashMap::new(),
            attributes: HashMap::new(),
        }
    }

    pub fn add_relation(&mut self, relation: BlockRelation, block: usize) {
        self.related_blocks
            .entry(relation)
            .or_insert_with(Vec::new)
            .push(block);
    }

    pub fn get_relations(&self, relation: BlockRelation) -> Option<&Vec<usize>> {
        self.related_blocks.get(&relation)
    }

    pub fn add_attribute(&mut self, key: String, value: String) {
        self.attributes.insert(key, value);
    }

    pub fn get_attribute(&self, key: &str) -> Option<&String> {
        self.attributes.get(key)
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn ast_id(&self) -> usize {
        self.ast_id
    }

    pub fn kind(&self) -> BlockKind {
        self.kind
    }
}

#[derive(Debug, Clone)]
pub struct FunctionBlock {
    base: BlockBase,
    define: FunctionDefineBlock,
    stat: FunctionStatBlock,
}

#[derive(Debug, Clone)]
pub struct ClassBlock {
    base: BlockBase,
    define: ClassDefineBlock,
    methods: Vec<FunctionBlock>,
    fields: Vec<VariableBlock>,
}

#[derive(Debug, Clone)]
pub struct VariableBlock {
    base: BlockBase,
    var: Vec<StatementBlock>,
}

#[derive(Debug, Clone)]
pub struct ModuleBlock {
    base: BlockBase,
    exported_symbols: Vec<String>,
    imported_modules: Vec<BasicBlock>,
    functions: Vec<BasicBlock>,
    classes: Vec<BasicBlock>,
    variables: Vec<BasicBlock>,
}

#[derive(Debug, Clone)]
pub enum BasicBlock {
    Undefined,
    Function(Box<FunctionBlock>),
    Class(Box<ClassBlock>),
    Variable(Box<VariableBlock>),
    Module(Box<ModuleBlock>),
}
impl Default for BasicBlock {
    fn default() -> Self {
        BasicBlock::Undefined
    }
}

use std::collections::HashMap;

// Symbol binding result to track resolution status
#[derive(Debug, Clone)]
pub enum SymbolBindingResult {
    Success,
    NotFound(String),
    Ambiguous(Vec<String>),
}

// Symbol binding context to maintain state during traversal
#[derive(Debug)]
struct SymbolBindingContext {
    // Map symbol names to their definition nodes
    symbol_definitions: HashMap<String, Vec<usize>>,
    // Current scope stack for nested scopes
    scope_stack: Vec<usize>,
    // Unresolved symbols for error reporting
    unresolved_symbols: Vec<(usize, String)>,
}

impl SymbolBindingContext {
    fn new() -> Self {
        Self {
            symbol_definitions: HashMap::new(),
            scope_stack: Vec::new(),
            unresolved_symbols: Vec::new(),
        }
    }

    fn enter_scope(&mut self, scope_id: usize) {
        self.scope_stack.push(scope_id);
    }

    fn leave_scope(&mut self) {
        self.scope_stack.pop();
    }

    fn add_symbol_definition(&mut self, name: String, node_id: usize) {
        self.symbol_definitions
            .entry(name)
            .or_insert_with(Vec::new)
            .push(node_id);
    }

    fn find_symbol_definition(&self, name: &str) -> Option<&Vec<usize>> {
        self.symbol_definitions.get(name)
    }

    fn add_unresolved_symbol(&mut self, node_id: usize, name: String) {
        self.unresolved_symbols.push((node_id, name));
    }
}

impl AstTree {
    /// Bind symbols in the AST tree, resolving references and establishing connections
    pub fn bind_symbols(&mut self) -> SymbolBindingResult {
        let mut context = SymbolBindingContext::new();

        // Get the root node and arena
        let root_node = match &self.root {
            AstKindNode::Root(root) => root,
            _ => return SymbolBindingResult::NotFound("No root node found".to_string()),
        };

        let arena = root_node.arena.clone();

        // First pass: collect all symbol definitions
        self.collect_symbol_definitions(&arena, 1, &mut context);

        // Second pass: resolve symbol references
        self.resolve_symbol_references(&arena, 1, &mut context);

        // Return binding result
        if context.unresolved_symbols.is_empty() {
            SymbolBindingResult::Success
        } else {
            let unresolved_names: Vec<String> = context
                .unresolved_symbols
                .into_iter()
                .map(|(_, name)| name)
                .collect();
            SymbolBindingResult::NotFound(format!("Unresolved symbols: {:?}", unresolved_names))
        }
    }

    /// First pass: collect all symbol definitions in the tree
    fn collect_symbol_definitions(
        &self,
        arena: &AstArenaShare<AstKindNode>,
        node_id: usize,
        context: &mut SymbolBindingContext,
    ) {
        let arena_ref = arena.borrow();
        let node = match arena_ref.get(node_id) {
            Some(node) => node,
            None => return,
        };

        match node {
            AstKindNode::Scope(scope_node) => {
                context.enter_scope(node_id);

                // Add symbols defined in this scope
                let symbol_name = &scope_node.scope.owner.name;
                if !symbol_name.is_empty() {
                    context.add_symbol_definition(symbol_name.clone(), node_id);
                }

                // Add all symbols in this scope to the definitions
                for (name, symbol) in &scope_node.scope.symbols {
                    if let Some(defined_node) = symbol.defined {
                        context.add_symbol_definition(name.clone(), defined_node);
                    }
                }
            }
            AstKindNode::IdentifierUse(id_node) => {
                // This is a potential definition site
                if let Some(defined_node) = id_node.symbol.defined {
                    context.add_symbol_definition(id_node.name.clone(), defined_node);
                }
            }
            _ => {}
        }

        // Recursively process children
        let children = node.get_base().children.clone();
        drop(arena_ref);

        for &child_id in &children {
            self.collect_symbol_definitions(arena, child_id, context);
        }

        // Leave scope if we entered one
        if matches!(node, AstKindNode::Scope(_)) {
            context.leave_scope();
        }
    }

    /// Second pass: resolve symbol references
    fn resolve_symbol_references(
        &mut self,
        arena: &AstArenaShare<AstKindNode>,
        node_id: usize,
        context: &mut SymbolBindingContext,
    ) {
        let children = {
            let arena_ref = arena.borrow();
            let node = match arena_ref.get(node_id) {
                Some(node) => node,
                None => return,
            };

            match node {
                AstKindNode::Scope(_) => {
                    context.enter_scope(node_id);
                }
                AstKindNode::IdentifierUse(id_node) => {
                    // Try to resolve this symbol reference
                    let symbol_name = &id_node.name;
                    match context.find_symbol_definition(symbol_name) {
                        Some(definitions) if !definitions.is_empty() => {
                            // For now, take the first definition (could be enhanced for scope resolution)
                            let definition_id = definitions[0];

                            // Update the symbol reference to point to its definition
                            drop(arena_ref);
                            let mut arena_mut = arena.borrow_mut();
                            if let Some(AstKindNode::IdentifierUse(ref mut id_node_mut)) =
                                arena_mut.get_mut(node_id)
                            {
                                id_node_mut.symbol.defined = Some(definition_id);
                            }
                            let children =
                                arena_mut.get(node_id).unwrap().get_base().children.clone();
                            drop(arena_mut);
                            return self.process_children(arena, &children, context);
                        }
                        _ => {
                            // Unresolved symbol
                            context.add_unresolved_symbol(node_id, symbol_name.clone());
                        }
                    }
                }
                _ => {}
            }

            node.get_base().children.clone()
        };

        self.process_children(arena, &children, context);

        // Leave scope if we entered one
        let arena_ref = arena.borrow();
        if let Some(node) = arena_ref.get(node_id) {
            if matches!(node, AstKindNode::Scope(_)) {
                context.leave_scope();
            }
        }
    }

    /// Helper function to process child nodes
    fn process_children(
        &mut self,
        arena: &AstArenaShare<AstKindNode>,
        children: &[usize],
        context: &mut SymbolBindingContext,
    ) {
        for &child_id in children {
            self.resolve_symbol_references(arena, child_id, context);
        }
    }

    /// Get all symbol definitions in the tree
    pub fn get_symbol_definitions(&self) -> HashMap<String, Vec<usize>> {
        let mut context = SymbolBindingContext::new();

        if let AstKindNode::Root(root) = &self.root {
            let arena = &root.arena;
            self.collect_symbol_definitions(arena, 1, &mut context);
        }

        context.symbol_definitions
    }

    /// Find all references to a given symbol
    pub fn find_symbol_references(&self, symbol_name: &str) -> Vec<usize> {
        let mut references = Vec::new();

        if let AstKindNode::Root(root) = &self.root {
            let arena = &root.arena;
            self.find_references_recursive(arena, 1, symbol_name, &mut references);
        }

        references
    }

    /// Helper function to find references recursively
    fn find_references_recursive(
        &self,
        arena: &AstArenaShare<AstKindNode>,
        node_id: usize,
        target_name: &str,
        references: &mut Vec<usize>,
    ) {
        let arena_ref = arena.borrow();
        let node = match arena_ref.get(node_id) {
            Some(node) => node,
            None => return,
        };

        // Check if this node references the target symbol
        match node {
            AstKindNode::IdentifierUse(id_node) => {
                if id_node.name == target_name {
                    references.push(node_id);
                }
            }
            _ => {}
        }

        // Recursively check children
        let children = node.get_base().children.clone();
        drop(arena_ref);

        for &child_id in &children {
            self.find_references_recursive(arena, child_id, target_name, references);
        }
    }

    /// Validate symbol bindings and return detailed information
    pub fn validate_symbol_bindings(&self) -> Vec<(usize, String, SymbolBindingResult)> {
        let mut results = Vec::new();

        if let AstKindNode::Root(root) = &self.root {
            let arena = &root.arena;
            let context = SymbolBindingContext::new();
            self.validate_bindings_recursive(arena, 1, &context, &mut results);
        }

        results
    }

    /// Helper function to validate bindings recursively
    fn validate_bindings_recursive(
        &self,
        arena: &AstArenaShare<AstKindNode>,
        node_id: usize,
        context: &SymbolBindingContext,
        results: &mut Vec<(usize, String, SymbolBindingResult)>,
    ) {
        let arena_ref = arena.borrow();
        let node = match arena_ref.get(node_id) {
            Some(node) => node,
            None => return,
        };

        match node {
            AstKindNode::IdentifierUse(id_node) => {
                let symbol_name = &id_node.name;
                let result = match id_node.symbol.defined {
                    Some(_) => SymbolBindingResult::Success,
                    None => {
                        SymbolBindingResult::NotFound(format!("Symbol '{}' not bound", symbol_name))
                    }
                };
                results.push((node_id, symbol_name.clone(), result));
            }
            _ => {}
        }

        // Recursively check children
        let children = node.get_base().children.clone();
        drop(arena_ref);

        for &child_id in &children {
            self.validate_bindings_recursive(arena, child_id, context, results);
        }
    }
}

use std::collections::HashMap;

impl AstScope {
    fn add_symbol(&mut self, mangled_name: String, symbol: Box<AstSymbol>) {
        self.symbols.insert(mangled_name, symbol);
    }

    fn get_scope_path(&self) -> String {
        if self.owner.name.is_empty() {
            String::new()
        } else {
            self.owner.name.clone()
        }
    }

    fn get_full_scope_path(&self, parent_scopes: &[AstScope]) -> String {
        let mut path_components = Vec::new();

        // Collect parent scope names
        for scope in parent_scopes {
            if !scope.owner.name.is_empty() {
                path_components.push(scope.owner.name.clone());
            }
        }

        // Add current scope name
        if !self.owner.name.is_empty() {
            path_components.push(self.owner.name.clone());
        }

        path_components.join("::")
    }
}

impl<'a> AstSymbolCollecter<'a> {
    fn mangled_name(&self, name: &mut Box<AstNodeId>) {
        let symbol_name = &name.name;
        let symbol_kind = name.base.kind;

        // Generate mangled name based on scope stack and symbol properties
        let mangled = self.generate_mangled_name(symbol_name, symbol_kind, &name.symbol);

        // Update both the node and its symbol with the mangled name
        name.mangled_name = mangled.clone();
        name.symbol.mangled_name = mangled;
    }

    fn generate_mangled_name(
        &self,
        symbol_name: &str,
        kind: AstKind,
        symbol: &AstSymbol,
    ) -> String {
        let mut mangled = String::new();

        // Start with a prefix indicating this is a mangled name
        mangled.push_str("_Z");

        // Add scope path
        let scope_path = self.get_current_scope_path();
        if !scope_path.is_empty() {
            self.encode_scope_path(&mut mangled, &scope_path);
        }

        // Add symbol name with length prefix (similar to Itanium C++ ABI)
        self.encode_name(&mut mangled, symbol_name);

        // Add type information if available
        if let Some(ref type_symbol) = symbol.type_of {
            self.encode_type(&mut mangled, type_symbol);
        }

        // Add kind-specific suffixes
        match kind {
            AstKind::IdentifierDef => mangled.push_str("D"), // Definition
            AstKind::IdentifierUse => mangled.push_str("U"), // Use
            AstKind::IdentifierTypeDef => mangled.push_str("T"), // Type definition
            AstKind::IdentifierTypeUse => mangled.push_str("t"), // Type use
            AstKind::IdentifierFieldDef => mangled.push_str("F"), // Field definition
            AstKind::IdentifierFieldUse => mangled.push_str("f"), // Field use
            _ => {}                                          // No suffix for other kinds
        }

        // Handle overloads by adding a numeric suffix
        if !symbol.overloads.is_empty() {
            mangled.push_str(&format!("O{}", symbol.overloads.len()));
        }

        // Add hash of the full context for uniqueness if needed
        if self.needs_disambiguation(&mangled, symbol_name) {
            let hash = self.calculate_context_hash(symbol);
            mangled.push_str(&format!("H{:x}", hash));
        }

        mangled
    }

    fn get_current_scope_path(&self) -> String {
        let mut path_components = Vec::new();

        for scope in &self.scope_stack.scopes {
            if !scope.owner.name.is_empty() {
                path_components.push(scope.owner.name.clone());
            }
        }

        path_components.join("::")
    }

    fn encode_scope_path(&self, mangled: &mut String, scope_path: &str) {
        let components: Vec<&str> = scope_path.split("::").collect();

        // Add number of scope components
        mangled.push_str(&format!("N{}", components.len()));

        // Encode each component
        for component in components {
            self.encode_name(mangled, component);
        }

        mangled.push('E'); // End of nested name
    }

    fn encode_name(&self, mangled: &mut String, name: &str) {
        // Length-prefixed name encoding (like Itanium C++ ABI)
        mangled.push_str(&format!("{}{}", name.len(), name));
    }

    fn encode_type(&self, mangled: &mut String, type_symbol: &AstSymbol) {
        // Simple type encoding - can be expanded based on your type system
        mangled.push('_');

        if !type_symbol.name.is_empty() {
            self.encode_name(mangled, &type_symbol.name);
        } else {
            mangled.push_str("v"); // void/unknown type
        }
    }

    fn needs_disambiguation(&self, current_mangled: &str, symbol_name: &str) -> bool {
        // Check if there are potential naming conflicts in current scope
        if let Some(current_scope) = self.scope_stack.scopes.last() {
            let same_name_count = current_scope
                .symbols
                .values()
                .filter(|s| s.name == symbol_name)
                .count();

            return same_name_count > 1;
        }

        false
    }

    fn calculate_context_hash(&self, symbol: &AstSymbol) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        // Hash the symbol's origin position
        symbol.origin.row.hash(&mut hasher);
        symbol.origin.col.hash(&mut hasher);

        // Hash the current scope stack depth
        self.scope_stack.scopes.len().hash(&mut hasher);

        // Hash type information if available
        if let Some(ref type_symbol) = symbol.type_of {
            type_symbol.name.hash(&mut hasher);
        }

        hasher.finish()
    }

    /// Generate a simple mangled name for basic cases
    fn simple_mangled_name(&self, symbol_name: &str, scope_prefix: Option<&str>) -> String {
        match scope_prefix {
            Some(prefix) if !prefix.is_empty() => format!("{}::{}", prefix, symbol_name),
            _ => symbol_name.to_string(),
        }
    }

    /// Generate a fully qualified name (human-readable alternative to mangling)
    fn qualified_name(&self, symbol_name: &str) -> String {
        let scope_path = self.get_current_scope_path();
        if scope_path.is_empty() {
            symbol_name.to_string()
        } else {
            format!("{}::{}", scope_path, symbol_name)
        }
    }

    /// Resolve a symbol by its mangled name
    fn resolve_symbol_by_mangled_name(&self, mangled_name: &str) -> Option<&Box<AstSymbol>> {
        // Search through all scopes in the stack
        for scope in self.scope_stack.scopes.iter().rev() {
            if let Some(symbol) = scope.symbols.get(mangled_name) {
                return Some(symbol);
            }
        }
        None
    }

    /// Get all symbols with a given base name (before mangling)
    fn find_symbols_by_base_name(&self, base_name: &str) -> Vec<&Box<AstSymbol>> {
        let mut results = Vec::new();

        for scope in &self.scope_stack.scopes {
            for symbol in scope.symbols.values() {
                if symbol.name == base_name {
                    results.push(symbol);
                }
            }
        }

        results
    }
}

// Additional utility functions for mangling
impl AstSymbolCollecter<'_> {
    /// Create a mangled name for a function with parameters
    fn mangle_function_name(&self, func_name: &str, param_types: &[String]) -> String {
        let mut mangled = self.generate_basic_mangled_name(func_name);

        // Add parameter types
        if !param_types.is_empty() {
            mangled.push('P');
            for param_type in param_types {
                self.encode_name(&mut mangled, param_type);
            }
            mangled.push('E');
        }

        mangled
    }

    /// Create a mangled name for a method (function within a class/struct)
    fn mangle_method_name(
        &self,
        class_name: &str,
        method_name: &str,
        param_types: &[String],
    ) -> String {
        let mut mangled = String::from("_Z");

        // Encode class name
        mangled.push('N');
        self.encode_name(&mut mangled, class_name);
        self.encode_name(&mut mangled, method_name);
        mangled.push('E');

        // Add parameter types
        if !param_types.is_empty() {
            mangled.push('P');
            for param_type in param_types {
                self.encode_name(&mut mangled, param_type);
            }
            mangled.push('E');
        }

        mangled
    }

    fn generate_basic_mangled_name(&self, symbol_name: &str) -> String {
        let mut mangled = String::from("_Z");
        let scope_path = self.get_current_scope_path();

        if !scope_path.is_empty() {
            self.encode_scope_path(&mut mangled, &scope_path);
        }

        self.encode_name(&mut mangled, symbol_name);
        mangled
    }
}

// Extension to AstLanguage for identifier upgrading
impl AstLanguage {
    /// Upgrade identifier kind based on context (definition vs use, type vs value, etc.)
    fn upgrade_identifier(&self, token_id: u16) -> Option<AstKind> {
        // This would be language-specific logic
        // For Rust, you might want to distinguish between:
        match AstTokenRust::from_repr(token_id) {
            Some(AstTokenRust::identifier) => {
                // Context-dependent - would need more information to determine
                // if this is a definition, use, type, etc.
                // For now, return None to indicate no upgrade needed
                None
            }
            _ => None,
        }
    }
}
