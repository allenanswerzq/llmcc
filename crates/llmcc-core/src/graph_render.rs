//! Graph rendering module for producing DOT format output.
//!
//! This module transforms a `ProjectGraph` into DOT format for visualization.
//! Nodes are grouped hierarchically by crate/module/file into nested subgraph clusters.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write;

use crate::BlockId;
use crate::block::{BlockKind, BlockRelation};
use crate::graph::ProjectGraph;
use crate::symbol::SymKind;

// ============================================================================
// Types
// ============================================================================

/// Component grouping depth for architecture graph visualization.
///
/// Controls the level of abstraction in the architecture graph:
/// - Lower depths show high-level relationships (project/crate dependencies)
/// - Higher depths show detailed relationships (individual types and functions)
///
/// Hierarchy levels:
/// - Depth 0 (Project): Show project-level nodes with edges between projects
/// - Depth 1 (Crate): Show crate-level nodes with edges between crates
/// - Depth 2 (Module): Show module-level nodes with edges between modules
/// - Depth 3 (File): Show individual nodes (structs, functions) grouped by file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComponentDepth {
    /// Project level - aggregate all nodes per project, show project dependencies
    Project,
    /// Crate level - aggregate all nodes per crate, show crate dependencies
    Crate,
    /// Module level - aggregate all nodes per module, show module dependencies
    Module,
    /// File level - show individual nodes (structs, functions) with file clustering (default)
    #[default]
    File,
}

impl ComponentDepth {
    /// Convert from numeric depth (for CLI compatibility)
    pub fn from_number(n: usize) -> Self {
        match n {
            0 => Self::Project,
            1 => Self::Crate,
            2 => Self::Module,
            _ => Self::File,
        }
    }

    /// Convert to numeric depth
    pub fn as_number(&self) -> usize {
        match self {
            Self::Project => 0,
            Self::Crate => 1,
            Self::Module => 2,
            Self::File => 3,
        }
    }

    /// Check if this is an aggregated view (not showing individual nodes)
    pub fn is_aggregated(&self) -> bool {
        !matches!(self, Self::File)
    }

    /// Check if showing individual file-level detail
    pub fn shows_file_detail(&self) -> bool {
        matches!(self, Self::File)
    }
}

/// Node representation for rendering.
#[derive(Clone)]
pub struct RenderNode {
    pub block_id: BlockId,
    /// Display name (e.g., "User", "process")
    pub name: String,
    /// File location (e.g., "src/model/user.rs:42")
    pub location: Option<String>,
    /// Crate name from Cargo.toml (e.g., "sample")
    pub crate_name: Option<String>,
    /// Module path (e.g., "utils::helpers")
    pub module_path: Option<String>,
    /// File name (e.g., "lib.rs")
    pub file_name: Option<String>,
    /// Symbol kind (Struct, Trait, Enum, Function, Method)
    pub sym_kind: Option<SymKind>,
}

/// Edge with semantic labels.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RenderEdge {
    pub from_id: BlockId,
    pub to_id: BlockId,
    /// Semantic role of source (e.g., "caller", "struct")
    pub from_label: &'static str,
    /// Semantic role of target (e.g., "callee", "field")
    pub to_label: &'static str,
}

/// Hierarchical tree for organizing nodes by component path.
#[derive(Default)]
struct ComponentTree {
    /// Direct child nodes at this level (indices into nodes array)
    node_indices: Vec<usize>,
    /// Child component subtrees (name -> (level_type, subtree))
    /// level_type: "crate", "module", or "file"
    children: BTreeMap<String, (String, ComponentTree)>,
}

impl ComponentTree {
    /// Insert a node at the given path.
    /// `path` is a list of (name, level_type) pairs.
    fn insert(&mut self, path: &[(String, &'static str)], node_idx: usize) {
        if path.is_empty() {
            self.node_indices.push(node_idx);
        } else {
            let (name, level_type) = &path[0];
            let child = self
                .children
                .entry(name.clone())
                .or_insert_with(|| (level_type.to_string(), ComponentTree::default()));
            child.1.insert(&path[1..], node_idx);
        }
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Block kinds to include in architecture graph:
/// - Types (Class, Trait, Enum) - the building blocks
/// - Free functions (Func) - entry points and pipelines
/// - Constants that define behavior
///
/// NOTE: Methods are EXCLUDED - they are implementation details of types.
/// NOTE: Fields are EXCLUDED - we only show type composition edges (A contains B).
const ARCHITECTURE_KINDS: [BlockKind; 4] = [
    BlockKind::Class,
    BlockKind::Trait,
    BlockKind::Enum,
    BlockKind::Func,
    // BlockKind::Const,
];

/// Options for graph rendering.
#[derive(Debug, Clone, Default)]
pub struct RenderOptions {
    /// If true, show all nodes even those without edges.
    /// If false (default), only show nodes that have at least one edge.
    pub show_orphan_nodes: bool,
}

/// Render the project graph to DOT format for visualization.
///
/// - `depth`: Component abstraction level
///   - Project (0): Show project-level dependencies
///   - Crate (1): Show crate-level dependencies
///   - Module (2): Show module-level dependencies
///   - File (3): Show individual nodes with file clustering
pub fn render_graph(project: &ProjectGraph, depth: ComponentDepth) -> String {
    render_graph_with_options(project, depth, &RenderOptions::default())
}

/// Render the project graph to DOT format with custom options.
///
/// For aggregated views (depth < File), nodes are aggregated into components
/// and edges show dependencies between those components.
pub fn render_graph_with_options(
    project: &ProjectGraph,
    depth: ComponentDepth,
    options: &RenderOptions,
) -> String {
    let nodes = collect_nodes(project);

    if nodes.is_empty() {
        return "digraph G {\n}\n".to_string();
    }

    let node_set: HashSet<BlockId> = nodes.iter().map(|n| n.block_id).collect();
    let edges = collect_edges(project, &node_set);

    // For aggregated views, aggregate nodes and edges
    if depth.is_aggregated() {
        return render_aggregated_graph(&nodes, &edges, depth);
    }

    // For file-level detail, use existing clustered rendering
    let mut filtered_nodes = nodes;

    // Filter out orphan nodes (nodes without edges) unless explicitly requested
    if !options.show_orphan_nodes {
        let connected_nodes: HashSet<BlockId> =
            edges.iter().flat_map(|e| [e.from_id, e.to_id]).collect();
        filtered_nodes.retain(|n| connected_nodes.contains(&n.block_id));
    }

    if filtered_nodes.is_empty() {
        return "digraph G {\n}\n".to_string();
    }

    let tree = build_component_tree(&filtered_nodes, depth);
    render_dot(&filtered_nodes, &edges, &tree)
}

// ============================================================================
// Node & Edge Collection
// ============================================================================

/// Collect nodes for architecture graph.
/// Includes: Types (Struct, Trait, Enum) and PUBLIC free functions.
/// Excludes: Methods and private/internal functions.
fn collect_nodes(project: &ProjectGraph) -> Vec<RenderNode> {
    let all_blocks = project.cc.get_all_blocks();

    let mut nodes: Vec<RenderNode> = all_blocks
        .into_iter()
        .filter_map(|(block_id, unit_index, name_opt, kind)| {
            // Skip kinds not in architecture view
            if !ARCHITECTURE_KINDS.contains(&kind) {
                return None;
            }

            let unit = project.cc.compile_unit(unit_index);
            let block = unit.bb(block_id);

            let display_name = name_opt
                .clone()
                .or_else(|| {
                    block
                        .base()
                        .and_then(|base| base.opt_get_name().map(|s| s.to_string()))
                })
                .unwrap_or_else(|| format!("{}:{}", kind, block_id.as_u32()));

            // Skip methods - they are implementation details, not architectural
            // Check BlockFunc::is_method() which is set during graph building
            if let Some(func_block) = block.as_func()
                && func_block.is_method()
            {
                return None;
            }

            // Get symbol info for visibility check
            let symbol_opt = block
                .opt_node()
                .and_then(|node| node.as_scope())
                .and_then(|scope_node| scope_node.opt_scope())
                .and_then(|scope| scope.opt_symbol());

            let sym_kind = symbol_opt.map(|s| s.kind());

            let raw_path = unit
                .file_path()
                .or_else(|| unit.file().path())
                .unwrap_or("<unknown>");

            let file_bytes = unit.file().content();
            let location = block
                .opt_node()
                .map(|node| {
                    let line = byte_to_line(file_bytes, node.start_byte());
                    format!("{raw_path}:{line}")
                })
                .or(Some(raw_path.to_string()));

            // Get crate_name and module_path from BlockRoot of this unit
            let (crate_name, module_path, file_name) = unit
                .root_block()
                .and_then(|root| root.as_root())
                .map(|root| {
                    let crate_name = root.get_crate_name();
                    let module_path = root.get_module_path();
                    let file_name = root.file_name.clone();
                    (crate_name, module_path, file_name)
                })
                .unwrap_or((None, None, None));

            Some(RenderNode {
                block_id,
                name: display_name.clone(),
                location,
                crate_name,
                module_path,
                file_name,
                sym_kind,
            })
        })
        .collect();

    // Sort by name for deterministic output
    nodes.sort_by(|a, b| a.name.cmp(&b.name));
    nodes
}

/// Collect edges from related_map for nodes in the graph.
/// Produces rich edge types:
/// - struct → field: Type composition
/// - caller → callee: Function calls
/// - input → func: Parameter types
/// - func → output: Return types
/// - trait → impl: Trait implementations
/// - bound → generic: Trait bounds on generics
fn collect_edges(project: &ProjectGraph, node_set: &HashSet<BlockId>) -> BTreeSet<RenderEdge> {
    let mut edges = BTreeSet::new();

    // Helper to get block kind
    let get_kind = |id: BlockId| -> Option<BlockKind> {
        let index = (id.as_u32() as usize).saturating_sub(1);
        project.cc.block_arena.bb().get(index).map(|b| b.kind())
    };

    // Helper to recursively collect type references from fields (including nested variant fields)
    fn collect_field_types(project: &ProjectGraph, field_id: BlockId, types: &mut Vec<BlockId>) {
        // Get direct TypeOf relations
        let field_types = project
            .cc
            .related_map
            .get_related(field_id, BlockRelation::TypeOf);
        types.extend(field_types);

        // Recursively check nested fields (for enum variants with struct-like fields)
        let nested_fields = project
            .cc
            .related_map
            .get_related(field_id, BlockRelation::HasField);
        for nested_field_id in nested_fields {
            collect_field_types(project, nested_field_id, types);
        }
    }

    for &block_id in node_set {
        let block_kind = get_kind(block_id);

        // 1. Field type → Struct/Enum (field_type is used by struct/enum)
        // Recursively looks at nested fields (e.g., enum variants with struct-like fields)
        let fields = project
            .cc
            .related_map
            .get_related(block_id, BlockRelation::HasField);
        for field_id in fields {
            let mut field_types = Vec::new();
            collect_field_types(project, field_id, &mut field_types);
            for type_id in field_types {
                if node_set.contains(&type_id) && block_id != type_id {
                    // Use actual block kind for to_label
                    let to_label = match block_kind {
                        Some(BlockKind::Enum) => "enum",
                        _ => "struct",
                    };
                    edges.insert(RenderEdge {
                        from_id: type_id,
                        to_id: block_id,
                        from_label: "field_type",
                        to_label,
                    });
                }
            }

            // 1b. Field type arguments → Field's generic type
            // For `data: Triple<User, Error>`, creates edges: User → Triple, Error → Triple
            // Only creates edges when the field's type is in node_set (a defined generic struct)
            let field_index = (field_id.as_u32() as usize).saturating_sub(1);
            let bb = project.cc.block_arena.bb();
            let Some(field_block) = bb.get(field_index) else {
                continue;
            };
            let Some(field) = field_block.as_field() else {
                continue;
            };
            let Some(field_type_id) = field.get_type_ref() else {
                continue;
            };
            // Skip if field's type is the containing struct/enum itself
            // This happens for enum variants where type_ref points to the enum
            if field_type_id == block_id {
                continue;
            }
            // Only create type_arg edges if field's type is in node_set
            if !node_set.contains(&field_type_id) {
                continue;
            }
            // Check that the field type is a Class/Enum (not a simple type)
            let field_type_kind = get_kind(field_type_id);
            if field_type_kind != Some(BlockKind::Class) && field_type_kind != Some(BlockKind::Enum)
            {
                continue;
            }
            // Get the field's nested_types (e.g., User, Error)
            let Some(field_sym) = field.base.symbol else {
                continue;
            };
            let Some(nested_types) = field_sym.nested_types() else {
                continue;
            };
            // Check if field_type_id is itself one of the nested types
            // This happens when outer generic (e.g., HashMap) isn't defined, and type_ref
            // falls back to the first resolved type arg. In this case, create type_dep edges
            // from ALL nested types to the containing struct (not type_arg -> generic).
            let field_type_is_nested = nested_types.iter().any(|&nested_id| {
                project
                    .cc
                    .opt_get_symbol(nested_id)
                    .and_then(|sym| {
                        sym.type_of()
                            .and_then(|id| project.cc.opt_get_symbol(id))
                            .or(Some(sym))
                    })
                    .and_then(|sym| sym.block_id())
                    == Some(field_type_id)
            });
            if field_type_is_nested {
                // Outer generic not defined - remove field_type edge and add type_dep edges
                // Remove the field_type edge that was created in step 1
                let to_label = match block_kind {
                    Some(BlockKind::Enum) => "enum",
                    _ => "struct",
                };
                edges.remove(&RenderEdge {
                    from_id: field_type_id,
                    to_id: block_id,
                    from_label: "field_type",
                    to_label,
                });

                // Create type_dep edges for all nested types
                for nested_type_id in nested_types {
                    let Some(nested_sym) = project.cc.opt_get_symbol(nested_type_id) else {
                        continue;
                    };
                    let actual_sym = nested_sym
                        .type_of()
                        .and_then(|id| project.cc.opt_get_symbol(id))
                        .unwrap_or(nested_sym);
                    let Some(nested_block_id) = actual_sym.block_id() else {
                        continue;
                    };
                    if !node_set.contains(&nested_block_id) || nested_block_id == block_id {
                        continue;
                    }
                    let nested_kind = get_kind(nested_block_id);
                    if nested_kind != Some(BlockKind::Class) && nested_kind != Some(BlockKind::Enum)
                    {
                        continue;
                    }
                    edges.insert(RenderEdge {
                        from_id: nested_block_id,
                        to_id: block_id,
                        from_label: "type_dep",
                        to_label,
                    });
                }
                continue;
            }
            for nested_type_id in nested_types {
                // Resolve the nested type to its block
                let Some(nested_sym) = project.cc.opt_get_symbol(nested_type_id) else {
                    continue;
                };
                // Follow type_of chain
                let actual_sym = nested_sym
                    .type_of()
                    .and_then(|id| project.cc.opt_get_symbol(id))
                    .unwrap_or(nested_sym);
                let Some(nested_block_id) = actual_sym.block_id() else {
                    continue;
                };
                // Skip if same as field type or containing struct
                if !node_set.contains(&nested_block_id)
                    || nested_block_id == field_type_id
                    || nested_block_id == block_id
                {
                    continue;
                }
                edges.insert(RenderEdge {
                    from_id: nested_block_id,
                    to_id: field_type_id,
                    from_label: "type_arg",
                    to_label: "generic",
                });
            }
        }

        // 2. Function calls (caller → callee)
        let callees = project
            .cc
            .related_map
            .get_related(block_id, BlockRelation::Calls);
        for callee_id in callees {
            if node_set.contains(&callee_id) && block_id != callee_id {
                edges.insert(RenderEdge {
                    from_id: block_id,
                    to_id: callee_id,
                    from_label: "caller",
                    to_label: "callee",
                });
            }
        }

        // 3. Function parameters (input → func)
        // Walk: Func → HasParameters → Param → TypeOf → Type
        let params = project
            .cc
            .related_map
            .get_related(block_id, BlockRelation::HasParameters);
        for param_id in params {
            let param_types = project
                .cc
                .related_map
                .get_related(param_id, BlockRelation::TypeOf);
            for type_id in param_types {
                if node_set.contains(&type_id) && block_id != type_id {
                    edges.insert(RenderEdge {
                        from_id: type_id,
                        to_id: block_id,
                        from_label: "input",
                        to_label: "func",
                    });
                }
            }
        }

        // 4. Function return types (func → output)
        // Walk: Func → HasReturn → Return → TypeOf → Type
        let returns = project
            .cc
            .related_map
            .get_related(block_id, BlockRelation::HasReturn);
        for ret_id in returns {
            let ret_types = project
                .cc
                .related_map
                .get_related(ret_id, BlockRelation::TypeOf);
            for type_id in ret_types {
                if node_set.contains(&type_id) && block_id != type_id {
                    edges.insert(RenderEdge {
                        from_id: block_id,
                        to_id: type_id,
                        from_label: "func",
                        to_label: "output",
                    });
                }
            }
        }

        // 5. Trait implementations (trait → impl)
        // Walk: Type → HasImpl → Impl → Implements → Trait
        let impl_blocks = project
            .cc
            .related_map
            .get_related(block_id, BlockRelation::HasImpl);
        for impl_id in impl_blocks {
            let implements = project
                .cc
                .related_map
                .get_related(impl_id, BlockRelation::Implements);
            for trait_id in implements {
                if node_set.contains(&trait_id) && block_id != trait_id {
                    edges.insert(RenderEdge {
                        from_id: trait_id,
                        to_id: block_id,
                        from_label: "trait",
                        to_label: "impl",
                    });
                }
            }
        }

        // 6. Trait bounds on generics (bound → generic)
        // This would require tracking generic bounds - approximate via Uses
        if block_kind == Some(BlockKind::Trait) {
            // Find what uses this trait (could be as a bound)
            let used_by = project
                .cc
                .related_map
                .get_related(block_id, BlockRelation::UsedBy);
            for user_id in used_by {
                if node_set.contains(&user_id) && block_id != user_id {
                    let user_kind = get_kind(user_id);
                    // If the user is a function, struct, or trait, it might be using as a bound
                    if user_kind == Some(BlockKind::Func)
                        || user_kind == Some(BlockKind::Class)
                        || user_kind == Some(BlockKind::Trait)
                    {
                        edges.insert(RenderEdge {
                            from_id: block_id,
                            to_id: user_id,
                            from_label: "bound",
                            to_label: "generic",
                        });
                    }
                }
            }
        }

        // 7. Type dependencies (func → type) - from function body usage like Foo::new()
        // Skip if there's already an edge to the same target (e.g., output or input edge)
        if block_kind == Some(BlockKind::Func) {
            let uses = project
                .cc
                .related_map
                .get_related(block_id, BlockRelation::Uses);
            for type_id in uses {
                if node_set.contains(&type_id) && block_id != type_id {
                    let type_kind = get_kind(type_id);
                    // Only add edges to types (Class, Enum, Trait), not to other functions
                    if type_kind == Some(BlockKind::Class)
                        || type_kind == Some(BlockKind::Enum)
                        || type_kind == Some(BlockKind::Trait)
                    {
                        // Check if there's already an edge from this func to this type
                        let has_existing_edge = edges
                            .iter()
                            .any(|e| e.from_id == block_id && e.to_id == type_id);
                        if !has_existing_edge {
                            edges.insert(RenderEdge {
                                from_id: block_id,
                                to_id: type_id,
                                from_label: "func",
                                to_label: "type_dep",
                            });
                        }
                    }
                }
            }
        }

        // 8. Impl type arguments (type_arg → impl_target)
        // From `impl Trait<TypeArg> for Target`, create edge TypeArg → Target
        // Uses block.base.type_deps populated during link_impl from symbol's nested_types
        if block_kind == Some(BlockKind::Class) || block_kind == Some(BlockKind::Enum) {
            let index = (block_id.as_u32() as usize).saturating_sub(1);
            if let Some(block) = project.cc.block_arena.bb().get(index)
                && let Some(base) = block.base()
            {
                let type_deps = base.type_deps.read();
                for &type_arg_id in type_deps.iter() {
                    if node_set.contains(&type_arg_id) && block_id != type_arg_id {
                        let type_arg_kind = get_kind(type_arg_id);
                        // Only add edges from types (Class, Enum)
                        if type_arg_kind == Some(BlockKind::Class)
                            || type_arg_kind == Some(BlockKind::Enum)
                        {
                            // Check if there's already an edge from type_arg to this block
                            let has_existing_edge = edges
                                .iter()
                                .any(|e| e.from_id == type_arg_id && e.to_id == block_id);
                            if !has_existing_edge {
                                edges.insert(RenderEdge {
                                    from_id: type_arg_id,
                                    to_id: block_id,
                                    from_label: "type_arg",
                                    to_label: "impl",
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    edges
}

// ============================================================================
// Aggregated Graph Rendering
// ============================================================================

/// An aggregated component node (represents a crate, module, or project).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct AggregatedNode {
    /// Unique identifier for this component
    id: String,
    /// Display label
    label: String,
    /// Component type: "project", "crate", or "module"
    component_type: &'static str,
    /// Number of nodes aggregated into this component
    node_count: usize,
}
/// Get the component key for a node at a given depth level.
/// Returns (component_id, component_label, component_type).
fn get_component_key(node: &RenderNode, depth: ComponentDepth) -> (String, String, &'static str) {
    match depth {
        ComponentDepth::Project => {
            // All nodes belong to the same project
            ("project".to_string(), "project".to_string(), "project")
        }
        ComponentDepth::Crate => {
            let crate_name = node.crate_name.clone().unwrap_or_else(|| "unknown".to_string());
            let id = format!("crate_{}", sanitize_id(&crate_name));
            (id, crate_name, "crate")
        }
        ComponentDepth::Module => {
            let crate_name = node.crate_name.clone().unwrap_or_else(|| "unknown".to_string());
            let module_path = node.module_path.clone();

            // If there's a module path, use crate::module format
            // Otherwise, use crate::file format (file acts as implicit module)
            let (label, id) = if let Some(ref module) = module_path {
                let label = format!("{}::{}", crate_name, module);
                let id = format!("mod_{}_{}", sanitize_id(&crate_name), sanitize_id(module));
                (label, id)
            } else {
                // Use file name as implicit module
                let file_name = node.file_name.clone()
                    .map(|f| {
                        std::path::Path::new(&f)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or(&f)
                            .to_string()
                    })
                    .unwrap_or_else(|| "unknown".to_string());
                let label = format!("{}::{}", crate_name, file_name);
                let id = format!("mod_{}_{}", sanitize_id(&crate_name), sanitize_id(&file_name));
                (label, id)
            };
            (id, label, "module")
        }
        ComponentDepth::File => {
            // This shouldn't be called for File depth, but handle it gracefully
            let name = node.name.clone();
            let id = format!("node_{}", node.block_id.as_u32());
            (id, name, "node")
        }
    }
}

/// Sanitize a string for use as a DOT node ID.
fn sanitize_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

/// Render an aggregated graph where nodes represent components (crates/modules/projects)
/// and edges represent dependencies between those components.
fn render_aggregated_graph(
    nodes: &[RenderNode],
    edges: &BTreeSet<RenderEdge>,
    depth: ComponentDepth,
) -> String {
    // Build mapping from BlockId to component key
    let mut block_to_component: std::collections::HashMap<BlockId, String> =
        std::collections::HashMap::new();
    let mut component_nodes: BTreeMap<String, AggregatedNode> = BTreeMap::new();

    for node in nodes {
        let (id, label, component_type) = get_component_key(node, depth);
        block_to_component.insert(node.block_id, id.clone());

        component_nodes
            .entry(id.clone())
            .and_modify(|n| n.node_count += 1)
            .or_insert(AggregatedNode {
                id,
                label,
                component_type,
                node_count: 1,
            });
    }

    // Aggregate edges between components using dependency semantics.
    // For dependency graphs, we want "A depends on B" shown as A → B.
    // Some edge types need to be flipped to show proper dependency direction:
    // - "field_type → struct" should become "struct → field_type" (struct depends on type)
    // - "input → func" should become "func → input" (func depends on param type)
    // - "func → output" stays as-is (func depends on return type) - wait, this is already correct
    // - "caller → callee" stays as-is (caller depends on callee)
    let mut component_edges: BTreeMap<(String, String), usize> = BTreeMap::new();

    for edge in edges {
        let from_component = block_to_component.get(&edge.from_id);
        let to_component = block_to_component.get(&edge.to_id);

        if let (Some(from), Some(to)) = (from_component, to_component) {
            // Skip self-edges (edges within the same component)
            if from == to {
                continue;
            }

            // Flip edges to show dependency direction (dependent → dependency)
            let (dep_from, dep_to) = match (edge.from_label, edge.to_label) {
                // Type used as field → struct becomes struct → type
                ("field_type", "struct") => (to.clone(), from.clone()),
                // Type used as parameter → func becomes func → type
                ("input", "func") => (to.clone(), from.clone()),
                // Trait → impl becomes impl → trait (impl depends on trait)
                ("trait", "impl") => (to.clone(), from.clone()),
                // All other edges keep their direction
                _ => (from.clone(), to.clone()),
            };

            *component_edges.entry((dep_from, dep_to)).or_insert(0) += 1;
        }
    }

    // Filter to only show components that have edges
    let connected_components: HashSet<String> = component_edges
        .keys()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();

    let filtered_nodes: Vec<_> = component_nodes
        .values()
        .filter(|n| connected_components.contains(&n.id))
        .collect();

    // Render to DOT format
    let mut output = String::with_capacity(filtered_nodes.len() * 100 + component_edges.len() * 50);

    output.push_str("digraph architecture {\n");
    output.push_str("  rankdir=LR;\n");
    output.push_str("  node [shape=box];\n\n");

    // Add title based on depth
    let title = match depth {
        ComponentDepth::Project => "project graph",
        ComponentDepth::Crate => "crate graph",
        ComponentDepth::Module => "module graph",
        ComponentDepth::File => "architecture graph",
    };
    output.push_str(&format!("  label=\"{}\";\n", title));
    output.push_str("  labelloc=t;\n\n");

    // Render nodes
    for node in &filtered_nodes {
        let _ = writeln!(
            output,
            "  {}[label=\"{}\"];",
            node.id, node.label
        );
    }

    output.push('\n');

    // Render edges with weight as penwidth
    for ((from, to), _weight) in &component_edges {
        // Skip if either end is not in filtered nodes
        if !connected_components.contains(from) || !connected_components.contains(to) {
            continue;
        }

        // Scale penwidth based on weight (min 1, max 5)
        // let penwidth = ((*weight as f64).log2() + 1.0).min(5.0).max(1.0);

        let _ = writeln!(output, "  {} -> {};", from, to);
    }

    output.push_str("}\n");
    output
}

// ============================================================================
// DOT Rendering (File-level detail)
// ============================================================================

/// Build a ComponentTree from nodes based on crate/module/file hierarchy.
///
/// This is used for File-level depth where we show individual nodes
/// clustered by crate → module → file.
///
/// Each path element is (name, level_type) where level_type is "crate", "module", or "file".
fn build_component_tree(nodes: &[RenderNode], _depth: ComponentDepth) -> ComponentTree {
    let mut tree = ComponentTree::default();
    for (idx, node) in nodes.iter().enumerate() {
        let mut path: Vec<(String, &'static str)> = Vec::new();

        // Add crate level
        if let Some(ref crate_name) = node.crate_name {
            path.push((crate_name.clone(), "crate"));
        }

        // Add module level (if there's an explicit module path)
        if let Some(ref module) = node.module_path {
            path.push((module.clone(), "module"));
        }

        // Add file level
        if let Some(ref file) = node.file_name {
            // Extract just the filename from full path
            let file_name = std::path::Path::new(file)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(file);
            path.push((file_name.to_string(), "file"));
        }

        tree.insert(&path, idx);
    }
    tree
}

/// Render the graph to DOT format.
///
/// Hierarchy levels:
/// - Level 0: Project (wrapper for all crates)
/// - Level 1: Crate (from Cargo.toml name)
/// - Level 2: Module (from mod.rs structure)
/// - Level 3: File (source file)
fn render_dot(nodes: &[RenderNode], edges: &BTreeSet<RenderEdge>, tree: &ComponentTree) -> String {
    // Pre-allocate output buffer
    let estimated_size = nodes.len() * 150 + edges.len() * 80 + 200;
    let mut output = String::with_capacity(estimated_size);

    output.push_str("digraph architecture {\n");

    // Wrap everything in a Project cluster
    output.push_str("  subgraph cluster_project {\n");
    output.push_str("    label=\"project\";\n\n");

    // Render nodes grouped in clusters (crate → module → file)
    render_tree_recursive(&mut output, tree, nodes, 2);

    output.push_str("  }\n\n");

    // Render edges with labels (outside clusters)
    for edge in edges {
        let _ = writeln!(
            output,
            "  n{} -> n{} [from=\"{}\", to=\"{}\"];",
            edge.from_id.as_u32(),
            edge.to_id.as_u32(),
            edge.from_label,
            edge.to_label
        );
    }

    output.push_str("}\n");
    output
}

/// Recursively render the component tree as nested subgraph clusters.
///
/// Uses the stored level_type ("crate", "module", "file") for cluster naming.
fn render_tree_recursive(
    output: &mut String,
    tree: &ComponentTree,
    nodes: &[RenderNode],
    indent_level: usize,
) {
    // Render child subtrees (nested subgraphs)
    for (component_name, (level_type, subtree)) in &tree.children {
        // Use meaningful cluster names based on level type
        let cluster_id = match level_type.as_str() {
            "crate" => component_name.replace('-', "_"),
            "module" => component_name.replace('-', "_"),
            "file" => component_name.replace('.', "_"),
            _ => component_name.clone(),
        };

        write_indent(output, indent_level);
        let _ = writeln!(output, "subgraph cluster_{} {{", cluster_id);

        write_indent(output, indent_level);
        let _ = writeln!(output, "  label=\"{}\";", escape_dot_label(component_name));

        render_tree_recursive(output, subtree, nodes, indent_level + 1);

        write_indent(output, indent_level);
        output.push_str("}\n\n");
    }

    // Render nodes at this level
    let mut sorted_indices = tree.node_indices.clone();
    sorted_indices.sort_by(|&a, &b| {
        let node_a = &nodes[a];
        let node_b = &nodes[b];
        node_a
            .location
            .as_ref()
            .cmp(&node_b.location.as_ref())
            .then_with(|| node_a.name.cmp(&node_b.name))
            .then_with(|| node_a.block_id.as_u32().cmp(&node_b.block_id.as_u32()))
    });

    for idx in sorted_indices {
        let node = &nodes[idx];

        write_indent(output, indent_level);

        // Build node line
        let _ = write!(
            output,
            "n{}[label=\"{}\"",
            node.block_id.as_u32(),
            escape_dot_label(&node.name)
        );

        if let Some(location) = &node.location {
            let full_path = summarize_location(location);
            let _ = write!(output, ", full_path=\"{}\"", escape_dot_label(&full_path));
        }

        if let Some(sym_kind) = &node.sym_kind {
            let _ = write!(output, ", sym_ty=\"{:?}\"", sym_kind);
            let shape = shape_for_kind(Some(*sym_kind));
            if shape != "ellipse" {
                let _ = write!(output, ", shape={}", shape);
            }
        }

        output.push_str("];\n");
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Map SymKind to DOT shape.
fn shape_for_kind(kind: Option<SymKind>) -> &'static str {
    match kind {
        Some(SymKind::Struct | SymKind::Enum | SymKind::Trait) => "box",
        Some(SymKind::Field) => "plaintext",
        _ => "ellipse",
    }
}

/// Escape special characters for DOT labels.
fn escape_dot_label(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// Convert byte offset to line number.
fn byte_to_line(content: &[u8], byte_offset: usize) -> usize {
    content[..byte_offset.min(content.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}

/// Return the file path location for display.
/// Keep the full path so that test normalization can replace temp directories with $TMP.
fn summarize_location(location: &str) -> String {
    location.to_string()
}

/// Write indentation to output.
fn write_indent(output: &mut String, level: usize) {
    for _ in 0..level {
        output.push_str("  ");
    }
}
