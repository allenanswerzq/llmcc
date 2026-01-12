//! Dynamic language registry for multi-language support.
//!
//! This module provides runtime polymorphism over language handlers,
//! allowing any number of languages to be registered and used dynamically
//! without requiring compile-time generic parameters.

use std::collections::HashMap;
use std::sync::Arc;

use crate::lang_def::{LanguageTraitImpl, ParseTree};

/// Object-safe language handler trait.
/// This wraps static `LanguageTrait` methods into dynamic dispatch.
pub trait LanguageHandler: Send + Sync {
    /// Get the unique name of this language (e.g., "rust", "typescript")
    fn name(&self) -> &'static str;

    /// Get supported file extensions for this language
    fn extensions(&self) -> &'static [&'static str];

    /// Get the manifest file name (e.g., "Cargo.toml", "package.json")
    fn manifest_name(&self) -> &'static str;

    /// Check if a file extension is supported by this language
    fn supports_extension(&self, ext: &str) -> bool {
        self.extensions().contains(&ext)
    }

    /// Parse source code and return a generic parse tree
    fn parse(&self, text: &[u8]) -> Option<Box<dyn ParseTree>>;
}

/// A language handler implementation that wraps a LanguageTraitImpl.
pub struct LanguageHandlerImpl<L> {
    _marker: std::marker::PhantomData<L>,
    name: &'static str,
}

impl<L> LanguageHandlerImpl<L>
where
    L: LanguageTraitImpl,
{
    /// Create a new handler for the given language
    pub fn new(name: &'static str) -> Self {
        Self {
            _marker: std::marker::PhantomData,
            name,
        }
    }
}

impl<L> LanguageHandler for LanguageHandlerImpl<L>
where
    L: LanguageTraitImpl + Send + Sync + 'static,
{
    fn name(&self) -> &'static str {
        self.name
    }

    fn extensions(&self) -> &'static [&'static str] {
        L::supported_extensions()
    }

    fn manifest_name(&self) -> &'static str {
        L::manifest_name()
    }

    fn parse(&self, text: &[u8]) -> Option<Box<dyn ParseTree>> {
        L::parse(text)
    }
}

/// Registry of available language handlers.
pub struct LanguageRegistry {
    /// Map from language name to handler
    handlers: HashMap<&'static str, Arc<dyn LanguageHandler>>,
    /// Map from extension to handler
    extension_map: HashMap<&'static str, Arc<dyn LanguageHandler>>,
}

impl Default for LanguageRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            extension_map: HashMap::new(),
        }
    }

    /// Register a language handler
    pub fn register(&mut self, handler: Arc<dyn LanguageHandler>) {
        let name = handler.name();
        // Register by name
        self.handlers.insert(name, handler.clone());
        // Register by each extension
        for ext in handler.extensions() {
            self.extension_map.insert(*ext, handler.clone());
        }
    }

    /// Register a language by its LanguageTraitImpl type
    pub fn register_language<L>(&mut self, name: &'static str)
    where
        L: LanguageTraitImpl + Send + Sync + 'static,
    {
        let handler = Arc::new(LanguageHandlerImpl::<L>::new(name));
        self.register(handler);
    }

    /// Get a handler by language name
    pub fn get_by_name(&self, name: &str) -> Option<Arc<dyn LanguageHandler>> {
        self.handlers.get(name).cloned()
    }

    /// Get a handler by file extension
    pub fn get_by_extension(&self, ext: &str) -> Option<Arc<dyn LanguageHandler>> {
        self.extension_map.get(ext).cloned()
    }

    /// Get all registered extensions
    pub fn all_extensions(&self) -> Vec<&'static str> {
        self.extension_map.keys().copied().collect()
    }

    /// Get all registered language names
    pub fn all_languages(&self) -> Vec<&'static str> {
        self.handlers.keys().copied().collect()
    }

    /// Partition files by their language handler
    pub fn partition_files(&self, files: &[String]) -> HashMap<&'static str, Vec<String>> {
        let mut partitions: HashMap<&'static str, Vec<String>> = HashMap::new();

        for file in files {
            let path = std::path::Path::new(file);
            if let Some(ext) = path.extension().and_then(|e| e.to_str())
                && let Some(handler) = self.get_by_extension(ext)
            {
                partitions
                    .entry(handler.name())
                    .or_default()
                    .push(file.clone());
            }
        }

        partitions
    }

    /// Check if the registry has any handlers registered
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    /// Get the number of registered languages
    pub fn len(&self) -> usize {
        self.handlers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test with mock language handler
    struct MockHandler {
        name: &'static str,
        extensions: &'static [&'static str],
    }

    impl LanguageHandler for MockHandler {
        fn name(&self) -> &'static str {
            self.name
        }

        fn extensions(&self) -> &'static [&'static str] {
            self.extensions
        }

        fn manifest_name(&self) -> &'static str {
            "mock.toml"
        }

        fn parse(&self, _text: &[u8]) -> Option<Box<dyn ParseTree>> {
            None
        }
    }

    #[test]
    fn test_registry_basics() {
        let mut registry = LanguageRegistry::new();

        let rust_handler = Arc::new(MockHandler {
            name: "rust",
            extensions: &["rs"],
        });
        let ts_handler = Arc::new(MockHandler {
            name: "typescript",
            extensions: &["ts", "tsx"],
        });

        registry.register(rust_handler);
        registry.register(ts_handler);

        assert_eq!(registry.len(), 2);
        assert!(registry.get_by_name("rust").is_some());
        assert!(registry.get_by_extension("ts").is_some());
        assert!(registry.get_by_extension("tsx").is_some());
    }

    #[test]
    fn test_partition_files() {
        let mut registry = LanguageRegistry::new();

        let rust_handler = Arc::new(MockHandler {
            name: "rust",
            extensions: &["rs"],
        });
        let ts_handler = Arc::new(MockHandler {
            name: "typescript",
            extensions: &["ts"],
        });

        registry.register(rust_handler);
        registry.register(ts_handler);

        let files = vec![
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
            "src/index.ts".to_string(),
            "src/unknown.py".to_string(),
        ];

        let partitions = registry.partition_files(&files);

        assert_eq!(partitions.get("rust").map(|v| v.len()), Some(2));
        assert_eq!(partitions.get("typescript").map(|v| v.len()), Some(1));
        assert!(!partitions.contains_key("python"));
    }
}
