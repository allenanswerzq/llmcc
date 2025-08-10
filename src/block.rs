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
