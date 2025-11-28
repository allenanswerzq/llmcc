use parking_lot::RwLock;
use std::collections::HashMap;

use crate::interner::{InternPool, InternedStr};
use crate::symbol::{SymKind, Symbol};

#[derive(Debug, Default)]
struct SymbolTrieNode<'tcx> {
    children: HashMap<InternedStr, SymbolTrieNode<'tcx>>,
    symbols: Vec<&'tcx Symbol>,
}

impl<'tcx> SymbolTrieNode<'tcx> {
    fn child_mut(&mut self, key: InternedStr) -> &mut SymbolTrieNode<'tcx> {
        self.children.entry(key).or_default()
    }

    fn add_symbol(&mut self, symbol: &'tcx Symbol) {
        if self.symbols.iter().any(|existing| existing.id == symbol.id) {
            return;
        }
        self.symbols.push(symbol);
    }
}

#[derive(Debug, Default)]
pub struct SymbolTrie<'tcx> {
    root: RwLock<SymbolTrieNode<'tcx>>,
}

impl<'tcx> SymbolTrie<'tcx> {
    pub fn insert_symbol(&self, symbol: &'tcx Symbol, interner: &InternPool) {
        let fqn_key = symbol.fqn();
        let Some(fqn) = interner.resolve_owned(fqn_key) else {
            return;
        };
        if fqn.is_empty() {
            return;
        }

        let parts: Vec<InternedStr> = fqn
            .split("::")
            .filter(|segment| !segment.is_empty())
            .map(|segment| interner.intern(segment))
            .collect();
        if parts.is_empty() {
            return;
        }

        let mut guard = self.root.write();
        let mut node = &mut *guard;
        for segment in parts.iter().rev().copied() {
            node = node.child_mut(segment);
        }
        node.add_symbol(symbol);
    }

    pub fn insert_alias_path(&self, parts: &[InternedStr], symbol: &'tcx Symbol) {
        if parts.is_empty() {
            return;
        }

        let mut guard = self.root.write();
        let mut node = &mut *guard;
        for segment in parts.iter().rev().copied() {
            node = node.child_mut(segment);
        }
        node.add_symbol(symbol);
    }

    fn matches_filters(
        symbol: &Symbol,
        kind_filters: Option<&[SymKind]>,
        unit_filters: Option<&[usize]>,
    ) -> bool {
        if let Some(filters) = kind_filters {
            if !filters.iter().any(|expected| symbol.kind() == *expected) {
                return false;
            }
        }

        if let Some(filters) = unit_filters {
            if !filters
                .iter()
                .any(|expected| symbol.unit_index() == Some(*expected))
            {
                return false;
            }
        }

        true
    }

    fn collect_symbols(
        &self,
        node: &SymbolTrieNode<'tcx>,
        kind_filters: Option<&[SymKind]>,
        unit_filters: Option<&[usize]>,
        out: &mut Vec<&'tcx Symbol>,
    ) {
        for symbol in node.symbols.iter().copied() {
            if Self::matches_filters(symbol, kind_filters, unit_filters) {
                out.push(symbol);
            }
        }

        for child in node.children.values() {
            self.collect_symbols(child, kind_filters, unit_filters, out);
        }
    }

    pub fn lookup_symbol_suffix(
        &self,
        suffix: &[InternedStr],
        kind_filters: Option<&[SymKind]>,
        unit_filters: Option<&[usize]>,
    ) -> Vec<&'tcx Symbol> {
        if suffix.is_empty() {
            return Vec::new();
        }

        let guard = self.root.read();
        let mut node: &SymbolTrieNode<'tcx> = &*guard;
        for segment in suffix {
            let Some(child) = node.children.get(segment) else {
                return Vec::new();
            };
            node = child;
        }

        let mut results = Vec::new();
        self.collect_symbols(node, kind_filters, unit_filters, &mut results);
        results
    }

    pub fn lookup_symbol_exact(
        &self,
        path: &[InternedStr],
        kind_filters: Option<&[SymKind]>,
        unit_filters: Option<&[usize]>,
    ) -> Vec<&'tcx Symbol> {
        if path.is_empty() {
            return Vec::new();
        }

        let guard = self.root.read();
        let mut node: &SymbolTrieNode<'tcx> = &*guard;
        for segment in path {
            let Some(child) = node.children.get(segment) else {
                return Vec::new();
            };
            node = child;
        }

        node.symbols
            .iter()
            .copied()
            .filter(|symbol| Self::matches_filters(symbol, kind_filters, unit_filters))
            .collect()
    }

    pub fn clear(&self) {
        *self.root.write() = SymbolTrieNode::default();
    }

    pub fn total_symbols(&self) -> usize {
        let guard = self.root.read();
        self.count_symbols(&guard)
    }

    pub fn symbols(&self) -> Vec<&'tcx Symbol> {
        let guard = self.root.read();
        let mut results = Vec::new();
        self.collect_symbols(&guard, None, None, &mut results);
        results
    }

    fn count_symbols(&self, node: &SymbolTrieNode<'tcx>) -> usize {
        let mut total = node.symbols.len();
        for child in node.children.values() {
            total += self.count_symbols(child);
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Arena, HirId};

    #[test]
    fn trie_inserts_and_resolves_suffix() {
        let arena: Arena = Arena::default();
        let interner = InternPool::default();
        let key_bar = interner.intern("fn_bar");
        let key_baz = interner.intern("fn_baz");
        let symbol_a = arena.alloc(Symbol::new(HirId(1), key_bar));
        let symbol_b = arena.alloc(Symbol::new(HirId(2), key_baz));
        symbol_a.set_fqn(interner.intern("module_a::module_b::struct_foo::fn_bar"));
        symbol_b.set_fqn(interner.intern("module_a::module_b::struct_foo::fn_baz"));

        let trie = SymbolTrie::default();
        trie.insert_symbol(symbol_a, &interner);
        trie.insert_symbol(symbol_b, &interner);

        let suffix = trie.lookup_symbol_suffix(&[key_bar], None, None);
        assert_eq!(suffix.len(), 1);
        assert_eq!(suffix[0].id, symbol_a.id);

        let exact = trie.lookup_symbol_exact(
            &[
                key_baz,
                interner.intern("struct_foo"),
                interner.intern("module_b"),
                interner.intern("module_a"),
            ],
            None,
            None,
        );
        assert_eq!(exact.len(), 1);
        assert_eq!(exact[0].id, symbol_b.id);
    }
}
