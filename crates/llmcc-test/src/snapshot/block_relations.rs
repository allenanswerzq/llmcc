//! Block relations snapshot capture and rendering.
//!
//! This module captures the relationships established by `connect_blocks()`
//! to verify impl-struct associations, function containment, and other
//! block relationships.

use super::{Snapshot, SnapshotContext};
use llmcc_core::block::BlockRelation;
use std::fmt::Write as _;

/// Snapshot of block relationships (impl targets, method containment, etc.).
#[derive(Clone)]
pub struct BlockRelationsSnapshot {
    entries: Vec<RelationEntry>,
}

#[derive(Clone)]
struct RelationEntry {
    /// Block label like "u0:5"
    label: String,
    /// Block kind (e.g., "Impl", "Struct", "Fn")
    kind: String,
    /// Block name if available
    name: String,
    /// Relations grouped by type
    relations: Vec<(BlockRelation, Vec<String>)>,
}

impl Snapshot for BlockRelationsSnapshot {
    fn capture(ctx: SnapshotContext<'_>) -> Self {
        let mut entries = Vec::new();

        // Access related_map directly from CompileCtxt
        let related_map = &ctx.cc.related_map;

        for unit_index in 0..ctx.cc.files.len() {
            // Get all blocks in this unit
            for (_name_opt, kind, block_id) in ctx.cc.find_blocks_in_unit(unit_index) {
                let label = format!("u{}:{}", unit_index, block_id.as_u32());

                // Get block name
                let name = ctx
                    .cc
                    .get_block_info(block_id)
                    .and_then(|(_, n, _)| n)
                    .unwrap_or_default();

                // Collect all relation types
                let mut relations = Vec::new();

                // Check for ImplFor relation (impl -> type it implements for)
                let impl_for = related_map.get_related(block_id, BlockRelation::ImplFor);
                if !impl_for.is_empty() {
                    let labels: Vec<String> = impl_for
                        .iter()
                        .map(|id| format!("u{}:{}", unit_index, id.as_u32()))
                        .collect();
                    relations.push((BlockRelation::ImplFor, labels));
                }

                // Check for HasImpl relation (type <- impl blocks)
                let has_impl = related_map.get_related(block_id, BlockRelation::HasImpl);
                if !has_impl.is_empty() {
                    let labels: Vec<String> = has_impl
                        .iter()
                        .map(|id| format!("u{}:{}", unit_index, id.as_u32()))
                        .collect();
                    relations.push((BlockRelation::HasImpl, labels));
                }

                // Check for HasMethod relation (impl/trait/class -> methods)
                let has_method = related_map.get_related(block_id, BlockRelation::HasMethod);
                if !has_method.is_empty() {
                    let labels: Vec<String> = has_method
                        .iter()
                        .map(|id| format!("u{}:{}", unit_index, id.as_u32()))
                        .collect();
                    relations.push((BlockRelation::HasMethod, labels));
                }

                // Check for MethodOf relation (method <- impl/trait/class)
                let method_of = related_map.get_related(block_id, BlockRelation::MethodOf);
                if !method_of.is_empty() {
                    let labels: Vec<String> = method_of
                        .iter()
                        .map(|id| format!("u{}:{}", unit_index, id.as_u32()))
                        .collect();
                    relations.push((BlockRelation::MethodOf, labels));
                }

                // Check for Contains relation (structural parent -> child)
                let contains = related_map.get_related(block_id, BlockRelation::Contains);
                if !contains.is_empty() {
                    let labels: Vec<String> = contains
                        .iter()
                        .map(|id| format!("u{}:{}", unit_index, id.as_u32()))
                        .collect();
                    relations.push((BlockRelation::Contains, labels));
                }

                // Check for ContainedBy relation (child <- parent)
                let contained_by = related_map.get_related(block_id, BlockRelation::ContainedBy);
                if !contained_by.is_empty() {
                    let labels: Vec<String> = contained_by
                        .iter()
                        .map(|id| format!("u{}:{}", unit_index, id.as_u32()))
                        .collect();
                    relations.push((BlockRelation::ContainedBy, labels));
                }

                // Check for Calls relation (func -> called funcs)
                let calls = related_map.get_related(block_id, BlockRelation::Calls);
                if !calls.is_empty() {
                    let labels: Vec<String> = calls
                        .iter()
                        .map(|id| format!("u{}:{}", unit_index, id.as_u32()))
                        .collect();
                    relations.push((BlockRelation::Calls, labels));
                }

                // Check for CalledBy relation (func <- calling funcs)
                let called_by = related_map.get_related(block_id, BlockRelation::CalledBy);
                if !called_by.is_empty() {
                    let labels: Vec<String> = called_by
                        .iter()
                        .map(|id| format!("u{}:{}", unit_index, id.as_u32()))
                        .collect();
                    relations.push((BlockRelation::CalledBy, labels));
                }

                // Only include blocks that have relations
                if !relations.is_empty() {
                    entries.push(RelationEntry {
                        label,
                        kind: kind.to_string(),
                        name,
                        relations,
                    });
                }
            }
        }

        // Sort by label for deterministic output
        entries.sort_by(|a, b| a.label.cmp(&b.label));

        Self { entries }
    }

    fn render(&self) -> String {
        if self.entries.is_empty() {
            return "none\n".to_string();
        }

        let mut buf = String::new();

        for entry in &self.entries {
            // Header: label | kind | name
            let _ = writeln!(buf, "{} | {} | {}", entry.label, entry.kind, entry.name);

            // Relations, sorted by relation type
            let mut relations = entry.relations.clone();
            relations.sort_by(|a, b| format!("{:?}", a.0).cmp(&format!("{:?}", b.0)));

            for (relation, targets) in &relations {
                let mut sorted_targets = targets.clone();
                sorted_targets.sort();
                let _ = writeln!(buf, "  {:?} -> [{}]", relation, sorted_targets.join(", "));
            }
        }
        buf
    }

    fn normalize(text: &str) -> String {
        let canonical = text
            .replace("\r\n", "\n")
            .trim_end_matches('\n')
            .to_string();

        // Sort blocks and their relations
        let mut blocks: Vec<String> = Vec::new();
        let mut current_block = String::new();

        for line in canonical.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if trimmed.starts_with("u") && trimmed.contains('|') {
                // New block header
                if !current_block.is_empty() {
                    blocks.push(current_block.trim_end().to_string());
                }
                current_block = line.to_string();
                current_block.push('\n');
            } else if trimmed.starts_with("  ") || trimmed.contains("->") {
                // Relation line
                current_block.push_str(line);
                current_block.push('\n');
            }
        }

        if !current_block.is_empty() {
            blocks.push(current_block.trim_end().to_string());
        }

        blocks.sort();
        blocks.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_sorts_blocks() {
        let input = r#"u0:5 | Impl |
  ImplFor -> [u0:3]
u0:3 | Struct | Foo
  HasImpl -> [u0:5]"#;

        let normalized = BlockRelationsSnapshot::normalize(input);
        // u0:3 should come before u0:5
        assert!(normalized.find("u0:3").unwrap() < normalized.find("u0:5").unwrap());
    }
}
