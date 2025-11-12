use llmcc_core::ir::HirId;
use llmcc_core::symbol::SymbolKind;
use llmcc_descriptor::{PathQualifier, TypeExpr};

use crate::collector::CollectorCore;

pub(crate) struct TypeExprResolver<'a, 'tcx> {
    core: &'a mut CollectorCore<'tcx>,
    owner: HirId,
    kind: SymbolKind,
    is_global: bool,
    upsert: bool,
}

impl<'a, 'tcx> TypeExprResolver<'a, 'tcx> {
    pub(crate) fn new(
        core: &'a mut CollectorCore<'tcx>,
        owner: HirId,
        kind: SymbolKind,
        is_global: bool,
        upsert: bool,
    ) -> Self {
        Self {
            core,
            owner,
            kind,
            is_global,
            upsert,
        }
    }

    pub(crate) fn resolve(&mut self, expr: &TypeExpr) -> Option<usize> {
        match expr {
            TypeExpr::Path { qualifier, .. } => self.resolve_path(qualifier),
            TypeExpr::Reference { inner, .. } => self.resolve(inner),
            TypeExpr::Tuple(items) => items.iter().find_map(|item| self.resolve(item)),
            _ => None,
        }
    }

    fn resolve_path(&mut self, qualifier: &PathQualifier) -> Option<usize> {
        // Example: `impl crate::Foo` where the path segments resolve to a struct symbol.
        let parts: Vec<String> = qualifier.parts().to_vec();
        if parts.is_empty() {
            return None;
        }

        let part_refs: Vec<&str> = parts.iter().map(String::as_str).collect();

        let start_depth = match qualifier {
            // `super::` paths need to start their lookup from an ancestor scope.
            // Example: `impl super::outer::Widget` should skip the current module scope first.
            PathQualifier::Super { levels, .. } => {
                let depth = self.core.scope_depth();
                if depth == 0 {
                    None
                } else {
                    let levels = (*levels as usize).min(depth);
                    depth.checked_sub(levels).filter(|d| *d > 0)
                }
            }
            _ => None,
        };

        // Try resolving the full segmented path relative to the inferred scope depth.
        // Example: `impl codex::workspace::Sandbox` walks `["codex","workspace","Sandbox"]`.
        if let Some(idx) =
            self.core
                .lookup_from_scopes_with_parts(&part_refs, self.kind, start_depth)
        {
            return Some(idx);
        }

        // Fall back to matching the canonical FQN we have already recorded.
        // Example: when `parts` resolve to `"crate::outer::Widget"` exactly as stored.
        let canonical = parts.join("::");
        if !canonical.is_empty() {
            if let Some((idx, _)) = self
                .core
                .symbols()
                .iter()
                .enumerate()
                .rev()
                .find(|(_, symbol)| symbol.kind == self.kind && symbol.fqn == canonical)
            {
                return Some(idx);
            }
        }

        // As a last resort use only the terminal segment.
        // Example: `impl Widget` inside the same module finds the nearest `Widget`.
        if let Some(name) = part_refs.last().copied() {
            if let Some(idx) = self.core.lookup_from_scopes_with(name, self.kind) {
                return Some(idx);
            }
        }

        if self.upsert {
            // We tried our best but couldn't find a matching symbol, then we insert a new one.
            let (idx, _fqn) = self.core.insert_symbol(
                self.owner,
                parts.last().unwrap(),
                self.kind,
                self.is_global,
            );
            return Some(idx);
        }

        None
    }
}
