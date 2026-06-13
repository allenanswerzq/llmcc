//! Node and edge collection from ProjectGraph.

use std::collections::{BTreeSet, HashSet};

use rayon::prelude::*;

use llmcc_core::BlockId;
use llmcc_core::block::{BlockKind, BlockRelation};
use llmcc_core::graph::ProjectGraph;

use crate::types::{ARCHITECTURE_KINDS, RenderEdge, RenderNode};

/// Collect nodes for architecture graph.
///
/// Includes:
/// - Types (Struct, Trait, Enum) - the building blocks
/// - Public free functions - entry points and pipelines
///
/// Excludes:
/// - Methods (implementation details of types)
/// - Fields (we only show type composition edges)
/// - Private/internal functions
pub fn collect_nodes(project: &ProjectGraph) -> Vec<RenderNode> {
    let all_blocks = project.context().blocks();

    let mut nodes: Vec<RenderNode> = all_blocks
        .into_par_iter()
        .filter_map(|entry| {
            let block_id = entry.block_id;
            let unit_index = entry.unit_index;
            let kind = entry.kind;

            // Skip kinds not in architecture view
            if !ARCHITECTURE_KINDS.contains(&kind) {
                return None;
            }

            let unit = project.context().compile_unit(unit_index);
            let block = unit.block(block_id);

            let display_name = entry
                .name
                .or_else(|| block.try_name().map(|name| name.to_string()))
                .unwrap_or_else(|| format!("{}:{}", kind, block_id.as_u32()));

            // Skip methods - they are implementation details, not architectural
            if let Some(func_block) = block.as_func()
                && func_block.is_method()
            {
                return None;
            }

            // Get symbol info for visibility check
            let symbol_opt = block.node().try_scope_symbol();

            let sym_kind = symbol_opt.map(|s| s.kind());

            let raw_path = unit
                .file_path()
                .or_else(|| unit.file().path())
                .unwrap_or("<unknown>");

            let location = Some(format!("{}:{}", raw_path, block.node().start_line()));

            // Get crate_name and module_path from BlockRoot of this unit
            let (crate_name, crate_root, module_path, module_root, file_name) = unit
                .root_block()
                .and_then(|root| root.as_root())
                .map(|root| {
                    let crate_name = root.crate_name();
                    let crate_root = root.crate_root();
                    let module_path = root.module_path();
                    let module_root = root.module_root();
                    let file_name = root.file_name.clone();
                    (crate_name, crate_root, module_path, module_root, file_name)
                })
                .unwrap_or((None, None, None, None, None));

            Some(RenderNode {
                block_id,
                name: display_name,
                location,
                crate_name,
                crate_root,
                module_path,
                module_root,
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
///
/// Produces rich edge types:
/// - struct → field: Type composition
/// - caller → callee: Function calls
/// - input → func: Parameter types
/// - func → output: Return types
/// - trait → impl: Trait implementations
/// - type_arg → generic: Generic type arguments
/// - type_dep → type: Type dependencies from impl blocks
pub fn collect_edges(project: &ProjectGraph, node_set: &HashSet<BlockId>) -> BTreeSet<RenderEdge> {
    let get_kind = |id: BlockId| -> Option<BlockKind> {
        project.context().try_block(id).map(|block| block.kind())
    };

    // Collect edges in parallel for each block
    let node_vec: Vec<_> = node_set.iter().copied().collect();

    let edges: Vec<BTreeSet<RenderEdge>> = node_vec
        .into_par_iter()
        .map(|block_id| {
            let mut local_edges = BTreeSet::new();
            let block_kind = get_kind(block_id);

            // 1. Field types
            collect_field_edges(
                project,
                block_id,
                block_kind,
                node_set,
                &mut local_edges,
                get_kind,
            );

            // 2. Function calls
            collect_call_edges(project, block_id, node_set, &mut local_edges);

            // 3. Function parameters
            collect_param_edges(project, block_id, node_set, &mut local_edges);

            // 4. Function return types
            collect_return_edges(project, block_id, node_set, &mut local_edges);

            // 5. Trait implementations
            collect_impl_edges(project, block_id, node_set, &mut local_edges, get_kind);

            // 6. Inheritance (extends for classes/interfaces/traits)
            if block_kind == Some(BlockKind::Trait)
                || block_kind == Some(BlockKind::Interface)
                || block_kind == Some(BlockKind::Class)
            {
                collect_extends_edges(project, block_id, node_set, &mut local_edges);
            }

            // 7. Type dependencies from function bodies
            if block_kind == Some(BlockKind::Func) {
                collect_type_dep_edges(project, block_id, node_set, &mut local_edges, get_kind);
            }

            // 8. Impl type arguments and decorators
            if block_kind == Some(BlockKind::Class) || block_kind == Some(BlockKind::Enum) {
                collect_impl_type_arg_edges(
                    project,
                    block_id,
                    node_set,
                    &mut local_edges,
                    get_kind,
                );
                collect_decorator_edges(project, block_id, node_set, &mut local_edges, get_kind);
            }

            local_edges
        })
        .collect();

    // Merge all edge sets
    edges.into_iter().flatten().collect()
}

// Edge Collection Helpers

fn collect_field_types(project: &ProjectGraph, field_id: BlockId, types: &mut Vec<BlockId>) {
    let field_types = project
        .context()
        .block_relations()
        .related(field_id, BlockRelation::TypeOf);
    types.extend(field_types);

    let nested_fields = project
        .context()
        .block_relations()
        .related(field_id, BlockRelation::HasField);
    for nested_field_id in nested_fields {
        collect_field_types(project, nested_field_id, types);
    }
}

fn collect_field_edges<F>(
    project: &ProjectGraph,
    block_id: BlockId,
    block_kind: Option<BlockKind>,
    node_set: &HashSet<BlockId>,
    edges: &mut BTreeSet<RenderEdge>,
    get_kind: F,
) where
    F: Fn(BlockId) -> Option<BlockKind>,
{
    let fields = project
        .context()
        .block_relations()
        .related(block_id, BlockRelation::HasField);

    for field_id in fields {
        let mut field_types = Vec::new();
        collect_field_types(project, field_id, &mut field_types);

        for type_id in &field_types {
            if node_set.contains(type_id) && block_id != *type_id {
                let to_label = match block_kind {
                    Some(BlockKind::Enum) => "enum",
                    _ => "struct",
                };
                edges.insert(RenderEdge {
                    from_id: *type_id,
                    to_id: block_id,
                    from_label: "field_type",
                    to_label,
                });
            }
        }

        // Handle field type arguments
        let Some(field_block) = project.context().try_block(field_id) else {
            continue;
        };
        let Some(field) = field_block.as_field() else {
            continue;
        };
        let Some(field_type_id) = field.type_ref() else {
            continue;
        };
        if field_type_id == block_id || !node_set.contains(&field_type_id) {
            continue;
        }

        let field_type_kind = get_kind(field_type_id);
        if field_type_kind != Some(BlockKind::Class) && field_type_kind != Some(BlockKind::Enum) {
            continue;
        }

        let Some(field_sym) = field.base.symbol else {
            continue;
        };
        let Some(nested_types) = field_sym.nested_types() else {
            continue;
        };

        let field_type_is_nested = nested_types.iter().any(|&nested_id| {
            project
                .context()
                .try_symbol(nested_id)
                .and_then(|sym| {
                    sym.type_of()
                        .and_then(|id| project.context().try_symbol(id))
                        .or(Some(sym))
                })
                .and_then(|sym| sym.block_id())
                == Some(field_type_id)
        });

        if field_type_is_nested {
            // Outer generic not defined - remove field_type edge and add type_dep edges
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

            for nested_type_id in nested_types {
                let Some(nested_sym) = project.context().try_symbol(nested_type_id) else {
                    continue;
                };
                let actual_sym = nested_sym
                    .type_of()
                    .and_then(|id| project.context().try_symbol(id))
                    .unwrap_or(nested_sym);
                let Some(nested_block_id) = actual_sym.block_id() else {
                    continue;
                };
                if !node_set.contains(&nested_block_id) || nested_block_id == block_id {
                    continue;
                }
                let nested_kind = get_kind(nested_block_id);
                if nested_kind != Some(BlockKind::Class) && nested_kind != Some(BlockKind::Enum) {
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
            let Some(nested_sym) = project.context().try_symbol(nested_type_id) else {
                continue;
            };
            let actual_sym = nested_sym
                .type_of()
                .and_then(|id| project.context().try_symbol(id))
                .unwrap_or(nested_sym);
            let Some(nested_block_id) = actual_sym.block_id() else {
                continue;
            };
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
}

fn collect_call_edges(
    project: &ProjectGraph,
    block_id: BlockId,
    node_set: &HashSet<BlockId>,
    edges: &mut BTreeSet<RenderEdge>,
) {
    let callees = project
        .context()
        .block_relations()
        .related(block_id, BlockRelation::Calls);
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
}

fn collect_param_edges(
    project: &ProjectGraph,
    block_id: BlockId,
    node_set: &HashSet<BlockId>,
    edges: &mut BTreeSet<RenderEdge>,
) {
    let params = project
        .context()
        .block_relations()
        .related(block_id, BlockRelation::HasParameters);
    for param_id in params {
        let param_types = project
            .context()
            .block_relations()
            .related(param_id, BlockRelation::TypeOf);
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
}

fn collect_return_edges(
    project: &ProjectGraph,
    block_id: BlockId,
    node_set: &HashSet<BlockId>,
    edges: &mut BTreeSet<RenderEdge>,
) {
    let returns = project
        .context()
        .block_relations()
        .related(block_id, BlockRelation::HasReturn);
    for ret_id in returns {
        let ret_types = project
            .context()
            .block_relations()
            .related(ret_id, BlockRelation::TypeOf);
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
}

fn collect_impl_edges<F>(
    project: &ProjectGraph,
    block_id: BlockId,
    node_set: &HashSet<BlockId>,
    edges: &mut BTreeSet<RenderEdge>,
    get_kind: F,
) where
    F: Fn(BlockId) -> Option<BlockKind>,
{
    // Rust-style: struct -> impl block -> trait
    let impl_blocks = project
        .context()
        .block_relations()
        .related(block_id, BlockRelation::HasImpl);
    for impl_id in impl_blocks {
        let implements = project
            .context()
            .block_relations()
            .related(impl_id, BlockRelation::Implements);
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

    // TypeScript-style: class directly implements interface
    // Only create interface -> implements edge for TypeScript Interfaces, not Rust Traits
    let direct_implements = project
        .context()
        .block_relations()
        .related(block_id, BlockRelation::Implements);
    for interface_id in direct_implements {
        // Only TypeScript interfaces should get the "interface -> implements" label
        // Rust traits are handled above via impl blocks
        if node_set.contains(&interface_id)
            && block_id != interface_id
            && get_kind(interface_id) == Some(BlockKind::Interface)
        {
            edges.insert(RenderEdge {
                from_id: interface_id,
                to_id: block_id,
                from_label: "interface",
                to_label: "implements",
            });
        }
    }
}

/// Collect edges for trait inheritance (e.g., `interface Admin extends User`)
/// The edge goes from the child trait (Admin) to the parent trait (User).
fn collect_extends_edges(
    project: &ProjectGraph,
    block_id: BlockId,
    node_set: &HashSet<BlockId>,
    edges: &mut BTreeSet<RenderEdge>,
) {
    // Extends relation points from child (Admin) to parent (User)
    let extends = project
        .context()
        .block_relations()
        .related(block_id, BlockRelation::Extends);
    for parent_id in extends {
        if node_set.contains(&parent_id) && block_id != parent_id {
            edges.insert(RenderEdge {
                from_id: parent_id,
                to_id: block_id,
                from_label: "base",
                to_label: "extends",
            });
        }
    }
}

fn collect_type_dep_edges<F>(
    project: &ProjectGraph,
    block_id: BlockId,
    node_set: &HashSet<BlockId>,
    edges: &mut BTreeSet<RenderEdge>,
    get_kind: F,
) where
    F: Fn(BlockId) -> Option<BlockKind>,
{
    let uses = project
        .context()
        .block_relations()
        .related(block_id, BlockRelation::Uses);
    for type_id in uses {
        if node_set.contains(&type_id) && block_id != type_id {
            let type_kind = get_kind(type_id);
            if type_kind == Some(BlockKind::Class)
                || type_kind == Some(BlockKind::Enum)
                || type_kind == Some(BlockKind::Trait)
            {
                // Skip if this is a trait/interface used as a type bound
                // (a bound -> generic edge will be created from collect_bound_edges)
                if type_kind == Some(BlockKind::Trait) {
                    let used_by = project
                        .context()
                        .block_relations()
                        .related(type_id, BlockRelation::UsedBy);
                    if used_by.contains(&block_id) {
                        continue;
                    }
                }
                // Skip if there's already a more specific edge involving these nodes
                // Check outgoing edges (e.g., output)
                let has_outgoing_edge = edges
                    .iter()
                    .any(|e| e.from_id == block_id && e.to_id == type_id);
                // Check incoming edges (e.g., input -> func)
                let has_incoming_edge = edges
                    .iter()
                    .any(|e| e.from_id == type_id && e.to_id == block_id);
                if !has_outgoing_edge && !has_incoming_edge {
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

fn collect_impl_type_arg_edges<F>(
    project: &ProjectGraph,
    block_id: BlockId,
    node_set: &HashSet<BlockId>,
    edges: &mut BTreeSet<RenderEdge>,
    get_kind: F,
) where
    F: Fn(BlockId) -> Option<BlockKind>,
{
    let Some(block) = project.context().try_block(block_id) else {
        return;
    };
    let base = block.base();

    let type_deps = base.type_deps.read();
    for &type_arg_id in type_deps.iter() {
        if node_set.contains(&type_arg_id) && block_id != type_arg_id {
            let type_arg_kind = get_kind(type_arg_id);
            if type_arg_kind == Some(BlockKind::Class) || type_arg_kind == Some(BlockKind::Enum) {
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

/// Collect decorator edges for classes
/// When a class is decorated with @Component, @Injectable, etc., create edges
/// from the decorator function to the decorated class.
fn collect_decorator_edges<F>(
    project: &ProjectGraph,
    block_id: BlockId,
    node_set: &HashSet<BlockId>,
    edges: &mut BTreeSet<RenderEdge>,
    get_kind: F,
) where
    F: Fn(BlockId) -> Option<BlockKind>,
{
    // Decorators are stored in type_deps for the class
    // and have Uses/UsedBy relations
    let uses = project
        .context()
        .block_relations()
        .related(block_id, BlockRelation::Uses);
    for decorator_id in uses {
        if node_set.contains(&decorator_id) && block_id != decorator_id {
            let decorator_kind = get_kind(decorator_id);
            // Decorators are functions
            if decorator_kind == Some(BlockKind::Func) {
                edges.insert(RenderEdge {
                    from_id: decorator_id,
                    to_id: block_id,
                    from_label: "decorator",
                    to_label: "decorates",
                });
            }
        }
    }
}
