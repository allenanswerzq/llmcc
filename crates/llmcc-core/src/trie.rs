use std::collections::HashMap;

use crate::interner::{InternPool, InternedStr};
use crate::symbol::{Symbol, SymbolKind};

/// A trie structure to store and lookup symbols by their fully qualified names.
/// The trie is built in reverse order to facilitate suffix-based lookups.
/// Multiple same-named fqn symbols are stored in the same node, caller can use symbol
/// to do further disambiguation.
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
    root: SymbolTrieNode<'tcx>,
}

impl<'tcx> SymbolTrie<'tcx> {
    pub fn insert_symbol(&mut self, symbol: &'tcx Symbol, interner: &InternPool) {
        let fqn = symbol.fqn_name.read();
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

        let mut node = &mut self.root;
        for segment in parts.iter().rev().copied() {
            node = node.child_mut(segment);
        }
        node.add_symbol(symbol);
    }

    pub fn lookup_symbol_suffix(
        &self,
        suffix: &[InternedStr],
        kind_filter: Option<SymbolKind>,
        unit_filter: Option<usize>,
    ) -> Vec<&'tcx Symbol> {
        let mut node = &self.root;
        for segment in suffix {
            match node.children.get(segment) {
                Some(child) => node = child,
                None => return Vec::new(),
            }
        }
        let mut results = Vec::new();
        self.collect_symbols(node, kind_filter, unit_filter, &mut results);
        results
    }

    pub fn lookup_symbol_exact(&self, suffix: &[InternedStr]) -> Vec<&'tcx Symbol> {
        let mut node = &self.root;
        for segment in suffix {
            match node.children.get(segment) {
                Some(child) => node = child,
                None => return Vec::new(),
            }
        }
        node.symbols.clone()
    }

    pub fn clear(&mut self) {
        self.root = SymbolTrieNode::default();
    }

    #[allow(clippy::only_used_in_recursion)]
    fn collect_symbols(
        &self,
        node: &SymbolTrieNode<'tcx>,
        kind_filter: Option<SymbolKind>,
        unit_filter: Option<usize>,
        out: &mut Vec<&'tcx Symbol>,
    ) {
        for symbol in node.symbols.iter().copied() {
            let kind_matches = kind_filter.map_or(true, |expected| symbol.kind() == expected);
            let unit_matches =
                unit_filter.map_or(true, |expected| symbol.unit_index() == Some(expected));
            if kind_matches && unit_matches {
                out.push(symbol);
            }
        }
        for child in node.children.values() {
            self.collect_symbols(child, kind_filter, unit_filter, out);
        }
    }

    pub fn total_symbols(&self) -> usize {
        self.count_symbols(&self.root)
    }

    pub fn symbols(&self) -> Vec<&'tcx Symbol> {
        let mut results = Vec::new();
        self.collect_symbols(&self.root, None, None, &mut results);
        results
    }

    #[allow(clippy::only_used_in_recursion)]
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
        let name_bar = "fn_bar".to_string();
        let name_baz = "fn_baz".to_string();
        let key_bar = interner.intern(&name_bar);
        let key_baz = interner.intern(&name_baz);
        let symbol_a = arena.alloc(Symbol::new(HirId(1), name_bar.clone(), key_bar));
        let symbol_b = arena.alloc(Symbol::new(HirId(2), name_baz.clone(), key_baz));
        symbol_a.set_fqn(
            "module_a::module_b::struct_foo::fn_bar".to_string(),
            &interner,
        );
        symbol_b.set_fqn(
            "module_a::module_b::struct_foo::fn_baz".to_string(),
            &interner,
        );

        let mut trie = SymbolTrie::default();
        trie.insert_symbol(symbol_a, &interner);
        trie.insert_symbol(symbol_b, &interner);

        let suffix = trie.lookup_symbol_suffix(&[key_bar], None, None);
        assert_eq!(suffix.len(), 1);
        assert_eq!(suffix[0].id, symbol_a.id);

        let exact = trie.lookup_symbol_exact(&[
            key_baz,
            interner.intern("struct_foo"),
            interner.intern("module_b"),
            interner.intern("module_a"),
        ]);
        assert_eq!(exact.len(), 1);
        assert_eq!(exact[0].id, symbol_b.id);
    }
}
