//! A implementation of a trie (prefix tree) in Rust. where instead of brancing
//! based on characters, it branches based on a string, e.g
//!                     (root)
//!                   /      \
//!               "foo"      "bar"
//!             /   \         |
//!        "baz"     "qux"   "quux"
//!
use std::collections::HashMap;

/// A trie node that branches on strings instead of individual characters
#[derive(Debug, Clone)]
pub struct TrieNode {
    children: HashMap<String, TrieNode>,
    is_end: bool,
}

impl TrieNode {
    /// Creates a new empty trie node
    pub fn new() -> Self {
        TrieNode {
            children: HashMap::new(),
            is_end: false,
        }
    }
}

/// A trie (prefix tree) that branches on strings
#[derive(Debug, Clone)]
pub struct StringTrie {
    root: TrieNode,
}

impl StringTrie {
    /// Creates a new empty trie
    pub fn new() -> Self {
        StringTrie {
            root: TrieNode::new(),
        }
    }

    /// Inserts a path of strings into the trie
    ///
    /// # Example
    /// ```
    /// use llmcc_core::trie::StringTrie;
    /// let mut trie = StringTrie::new();
    /// trie.insert(&["foo", "baz"]);
    /// ```
    pub fn insert(&mut self, path: &[&str]) {
        let mut current = &mut self.root;

        for segment in path {
            current = current
                .children
                .entry(segment.to_string())
                .or_insert_with(TrieNode::new);
        }

        current.is_end = true;
    }

    /// Searches for an exact path in the trie
    ///
    /// Returns true if the path exists and is marked as a complete path
    pub fn search(&self, path: &[&str]) -> bool {
        let mut current = &self.root;

        for segment in path {
            match current.children.get(*segment) {
                Some(node) => current = node,
                None => return false,
            }
        }

        current.is_end
    }

    /// Checks if any path in the trie starts with the given prefix
    pub fn starts_with(&self, prefix: &[&str]) -> bool {
        let mut current = &self.root;

        for segment in prefix {
            match current.children.get(*segment) {
                Some(node) => current = node,
                None => return false,
            }
        }

        true
    }

    /// Returns all complete paths that start with the given prefix
    pub fn find_all_with_prefix(&self, prefix: &[&str]) -> Vec<Vec<String>> {
        let mut current = &self.root;

        // Navigate to the prefix node
        for segment in prefix {
            match current.children.get(*segment) {
                Some(node) => current = node,
                None => return Vec::new(),
            }
        }

        // Collect all paths from this node
        let mut results = Vec::new();
        let mut current_path = prefix.iter().map(|s| s.to_string()).collect();
        self.collect_paths(current, &mut current_path, &mut results);

        results
    }

    /// Helper function to recursively collect all complete paths
    fn collect_paths(&self, node: &TrieNode, current_path: &mut Vec<String>, results: &mut Vec<Vec<String>>) {
        if node.is_end {
            results.push(current_path.clone());
        }

        for (segment, child) in &node.children {
            current_path.push(segment.clone());
            self.collect_paths(child, current_path, results);
            current_path.pop();
        }
    }

    /// Deletes a path from the trie
    ///
    /// Returns true if the path was found and deleted
    pub fn delete(&mut self, path: &[&str]) -> bool {
        let (found, _) = Self::delete_recursive(&mut self.root, path, 0);
        found
    }

    // Returns (found, should_delete_this_node)
    fn delete_recursive(node: &mut TrieNode, path: &[&str], depth: usize) -> (bool, bool) {
        if depth == path.len() {
            if !node.is_end {
                return (false, false);
            }
            node.is_end = false;
            return (true, node.children.is_empty());
        }

        let segment = path[depth];
        if let Some(child) = node.children.get_mut(segment) {
            let (found, should_delete_child) = Self::delete_recursive(child, path, depth + 1);

            if should_delete_child {
                node.children.remove(segment);
            }

            let should_delete_this = !node.is_end && node.children.is_empty();
            return (found, should_delete_this);
        }

        (false, false)
    }

    /// Returns the number of complete paths in the trie
    pub fn count(&self) -> usize {
        self.count_recursive(&self.root)
    }

    fn count_recursive(&self, node: &TrieNode) -> usize {
        let mut count = if node.is_end { 1 } else { 0 };

        for child in node.children.values() {
            count += self.count_recursive(child);
        }

        count
    }
}

impl Default for StringTrie {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_search() {
        let mut trie = StringTrie::new();

        trie.insert(&["foo", "baz"]);
        trie.insert(&["foo", "qux"]);
        trie.insert(&["bar", "quux"]);

        assert!(trie.search(&["foo", "baz"]));
        assert!(trie.search(&["foo", "qux"]));
        assert!(trie.search(&["bar", "quux"]));
        assert!(!trie.search(&["foo"]));
        assert!(!trie.search(&["bar"]));
        assert!(!trie.search(&["baz"]));
    }

    #[test]
    fn test_starts_with() {
        let mut trie = StringTrie::new();

        trie.insert(&["foo", "baz", "test"]);
        trie.insert(&["foo", "qux"]);
        trie.insert(&["bar", "quux"]);

        assert!(trie.starts_with(&["foo"]));
        assert!(trie.starts_with(&["foo", "baz"]));
        assert!(trie.starts_with(&["bar"]));
        assert!(!trie.starts_with(&["baz"]));
        assert!(!trie.starts_with(&["foo", "nonexistent"]));
    }

    #[test]
    fn test_empty_trie() {
        let trie = StringTrie::new();

        assert!(!trie.search(&["foo"]));
        assert!(!trie.starts_with(&["foo"]));
        assert_eq!(trie.count(), 0);
    }

    #[test]
    fn test_single_segment_path() {
        let mut trie = StringTrie::new();

        trie.insert(&["root"]);

        assert!(trie.search(&["root"]));
        assert!(!trie.search(&[]));
    }

    #[test]
    fn test_overlapping_paths() {
        let mut trie = StringTrie::new();

        trie.insert(&["a", "b", "c"]);
        trie.insert(&["a", "b"]);
        trie.insert(&["a"]);

        assert!(trie.search(&["a"]));
        assert!(trie.search(&["a", "b"]));
        assert!(trie.search(&["a", "b", "c"]));
        assert!(trie.starts_with(&["a"]));
        assert!(trie.starts_with(&["a", "b"]));
    }

    #[test]
    fn test_find_all_with_prefix() {
        let mut trie = StringTrie::new();

        trie.insert(&["foo", "baz"]);
        trie.insert(&["foo", "qux"]);
        trie.insert(&["foo", "bar", "test"]);
        trie.insert(&["bar", "quux"]);

        let results = trie.find_all_with_prefix(&["foo"]);
        assert_eq!(results.len(), 3);
        assert!(results.contains(&vec!["foo".to_string(), "baz".to_string()]));
        assert!(results.contains(&vec!["foo".to_string(), "qux".to_string()]));
        assert!(results.contains(&vec!["foo".to_string(), "bar".to_string(), "test".to_string()]));

        let results = trie.find_all_with_prefix(&["bar"]);
        assert_eq!(results.len(), 1);
        assert!(results.contains(&vec!["bar".to_string(), "quux".to_string()]));

        let results = trie.find_all_with_prefix(&["nonexistent"]);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_delete() {
        let mut trie = StringTrie::new();

        trie.insert(&["foo", "baz"]);
        trie.insert(&["foo", "qux"]);
        trie.insert(&["bar", "quux"]);

        assert!(trie.delete(&["foo", "baz"]));
        assert!(!trie.search(&["foo", "baz"]));
        assert!(trie.search(&["foo", "qux"]));
        assert!(trie.starts_with(&["foo"]));

        assert!(trie.delete(&["foo", "qux"]));
        assert!(!trie.search(&["foo", "qux"]));
        assert!(!trie.starts_with(&["foo"]));

        assert!(!trie.delete(&["nonexistent"]));
    }

    #[test]
    fn test_delete_with_shared_prefix() {
        let mut trie = StringTrie::new();

        trie.insert(&["a", "b", "c"]);
        trie.insert(&["a", "b"]);

        assert!(trie.delete(&["a", "b", "c"]));
        assert!(!trie.search(&["a", "b", "c"]));
        assert!(trie.search(&["a", "b"]));
        assert!(trie.starts_with(&["a"]));
    }

    #[test]
    fn test_count() {
        let mut trie = StringTrie::new();

        assert_eq!(trie.count(), 0);

        trie.insert(&["foo", "baz"]);
        assert_eq!(trie.count(), 1);

        trie.insert(&["foo", "qux"]);
        assert_eq!(trie.count(), 2);

        trie.insert(&["bar", "quux"]);
        assert_eq!(trie.count(), 3);

        trie.delete(&["foo", "baz"]);
        assert_eq!(trie.count(), 2);
    }

    #[test]
    fn test_unicode_strings() {
        let mut trie = StringTrie::new();

        trie.insert(&["こんにちは", "世界"]);
        trie.insert(&["hello", "world"]);
        trie.insert(&["مرحبا", "عالم"]);

        assert!(trie.search(&["こんにちは", "世界"]));
        assert!(trie.search(&["hello", "world"]));
        assert!(trie.search(&["مرحبا", "عالم"]));
    }

    #[test]
    fn test_special_characters() {
        let mut trie = StringTrie::new();

        trie.insert(&["foo/bar", "baz\\qux"]);
        trie.insert(&["hello world", "test@123"]);

        assert!(trie.search(&["foo/bar", "baz\\qux"]));
        assert!(trie.search(&["hello world", "test@123"]));
    }
}