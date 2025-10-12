use std::collections::HashMap;

use crate::symbol::Symbol;

#[derive(Debug, Default)]
struct SymbolTrieNode<'tcx> {
    children: HashMap<String, SymbolTrieNode<'tcx>>,
    symbols: Vec<&'tcx Symbol>,
}

impl<'tcx> SymbolTrieNode<'tcx> {
    fn child_mut(&mut self, segment: &str) -> &mut SymbolTrieNode<'tcx> {
        self.children
            .entry(segment.to_string())
            .or_insert_with(SymbolTrieNode::default)
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
    pub fn insert_symbol(&mut self, symbol: &'tcx Symbol) {
        let fqn = symbol.fqn_name.borrow();
        if fqn.is_empty() {
            return;
        }

        let segments: Vec<&str> = fqn
            .split("::")
            .filter(|segment| !segment.is_empty())
            .collect();
        if segments.is_empty() {
            return;
        }

        let mut node = &mut self.root;
        for segment in segments.iter().rev() {
            node = node.child_mut(segment);
        }
        node.add_symbol(symbol);
    }

    pub fn lookup_symbol_suffix(&self, suffix: &[&str]) -> Vec<&'tcx Symbol> {
        let mut node = &self.root;
        for segment in suffix {
            match node.children.get(*segment) {
                Some(child) => node = child,
                None => return Vec::new(),
            }
        }
        let mut results = Vec::new();
        self.collect_symbols(node, &mut results);
        results
    }

    pub fn lookup_symbol_exact(&self, suffix: &[&str]) -> Vec<&'tcx Symbol> {
        let mut node = &self.root;
        for segment in suffix {
            match node.children.get(*segment) {
                Some(child) => node = child,
                None => return Vec::new(),
            }
        }
        node.symbols.clone()
    }

    pub fn clear(&mut self) {
        self.root = SymbolTrieNode::default();
    }

    fn collect_symbols(&self, node: &SymbolTrieNode<'tcx>, out: &mut Vec<&'tcx Symbol>) {
        out.extend(node.symbols.iter().copied());
        for child in node.children.values() {
            self.collect_symbols(child, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Arena, HirId};

    #[test]
    fn trie_inserts_and_resolves_suffix() {
        let arena: Arena = Arena::default();
        let symbol_a = arena.alloc(Symbol::new(
            HirId(1),
            "module_a::module_b::struct_foo::fn_bar".into(),
        ));
        let symbol_b = arena.alloc(Symbol::new(
            HirId(2),
            "module_a::module_b::struct_foo::fn_baz".into(),
        ));

        let mut trie = SymbolTrie::default();
        trie.insert_symbol(symbol_a);
        trie.insert_symbol(symbol_b);

        let suffix = trie.lookup_symbol_suffix(&["fn_bar"]);
        assert_eq!(suffix.len(), 1);
        assert_eq!(suffix[0].id, symbol_a.id);

        let exact = trie.lookup_symbol_exact(&["fn_baz", "struct_foo", "module_b", "module_a"]);
        assert_eq!(exact.len(), 1);
        assert_eq!(exact[0].id, symbol_b.id);
    }
}
