use parking_lot::RwLock;
use std::sync::Arc;

use string_interner::StringInterner;
use string_interner::backend::DefaultBackend;
use string_interner::symbol::DefaultSymbol;

/// Interned string symbol backed by a `StringInterner`.
pub type InternedStr = DefaultSymbol;

/// Inner implementation of the string interner.
#[derive(Debug)]
pub struct InternPoolInner {
    interner: RwLock<StringInterner<DefaultBackend>>,
}

impl InternPoolInner {
    /// Create a new interner.
    pub fn new() -> Self {
        Self {
            interner: RwLock::new(StringInterner::new()),
        }
    }

    /// Intern the provided string slice and return its symbol.
    #[inline]
    pub fn intern<S>(&self, value: S) -> InternedStr
    where
        S: AsRef<str>,
    {
        self.interner.write().get_or_intern(value.as_ref())
    }

    /// Intern multiple strings and return a vector of their symbols.
    pub fn intern_batch<S>(&self, values: impl IntoIterator<Item = S>) -> Vec<InternedStr>
    where
        S: AsRef<str>,
    {
        values.into_iter().map(|v| self.intern(v)).collect()
    }

    /// Resolve an interned symbol back into an owned string.
    ///
    /// Clones the underlying string from the interner to avoid lifetime issues.
    pub fn resolve_owned(&self, symbol: InternedStr) -> Option<String> {
        self.interner.read().resolve(symbol).map(|s| s.to_owned())
    }

    /// Resolve an interned symbol and apply a closure while the borrow is active.
    pub fn with_resolved<R, F>(&self, symbol: InternedStr, f: F) -> Option<R>
    where
        F: FnOnce(&str) -> R,
    {
        self.interner.read().resolve(symbol).map(f)
    }
}

impl Default for InternPoolInner {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared string interner used across the llmcc core.
///
/// Thread-safe wrapper around `InternPoolInner` using `Arc` for shared ownership.
#[derive(Clone, Debug)]
pub struct InternPool {
    inner: Arc<InternPoolInner>,
}

impl Default for InternPool {
    fn default() -> Self {
        Self::new()
    }
}

impl InternPool {
    /// Create a new shared interner pool.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(InternPoolInner::new()),
        }
    }

    /// Intern the provided string slice and return its symbol.
    pub fn intern<S>(&self, value: S) -> InternedStr
    where
        S: AsRef<str>,
    {
        self.inner.intern(value)
    }

    /// Intern multiple strings and return a vector of their symbols.
    pub fn intern_batch<S>(&self, values: impl IntoIterator<Item = S>) -> Vec<InternedStr>
    where
        S: AsRef<str>,
    {
        self.inner.intern_batch(values)
    }

    /// Resolve an interned symbol back into an owned string.
    ///
    /// Clones the underlying string from the interner to avoid lifetime issues.
    pub fn resolve_owned(&self, symbol: InternedStr) -> Option<String> {
        self.inner.resolve_owned(symbol)
    }

    /// Resolve an interned symbol and apply a closure while the borrow is active.
    pub fn with_resolved<R, F>(&self, symbol: InternedStr, f: F) -> Option<R>
    where
        F: FnOnce(&str) -> R,
    {
        self.inner.with_resolved(symbol, f)
    }

    /// Get the number of interned strings (for diagnostics).
    pub fn len(&self) -> usize {
        self.inner.interner.read().len()
    }

    /// Check if the pool is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rayon::prelude::*;

    #[test]
    fn interning_returns_stable_symbol() {
        let pool = InternPool::default();
        let first = pool.intern("foo");
        let second = pool.intern("foo");
        assert_eq!(
            first, second,
            "Interned symbols should be stable for the same string"
        );
    }

    #[test]
    fn resolve_owned_recovers_string() {
        let pool = InternPool::default();
        let sym = pool.intern("bar");
        let resolved = pool
            .resolve_owned(sym)
            .expect("symbol should resolve to a string");
        assert_eq!(resolved, "bar");
    }

    #[test]
    fn with_resolved_provides_borrowed_str() {
        let pool = InternPool::default();
        let sym = pool.intern("baz");
        let length = pool
            .with_resolved(sym, |s| s.len())
            .expect("symbol should resolve to a closure result");
        assert_eq!(length, 3);
    }

    #[test]
    fn intern_batch_interns_multiple_strings() {
        let pool = InternPool::default();
        let strings = vec!["apple", "banana", "cherry"];
        let symbols = pool.intern_batch(strings.clone());

        assert_eq!(symbols.len(), 3);

        // Verify each symbol resolves correctly
        for (i, sym) in symbols.iter().enumerate() {
            let resolved = pool.resolve_owned(*sym).expect("symbol should resolve");
            assert_eq!(resolved, strings[i]);
        }
    }

    #[test]
    fn intern_batch_with_duplicates() {
        let pool = InternPool::default();
        let strings = vec!["x", "y", "x", "z", "y"];
        let symbols = pool.intern_batch(strings);

        // Duplicates should map to the same symbol
        assert_eq!(
            symbols[0], symbols[2],
            "First and third 'x' should be the same symbol"
        );
        assert_eq!(
            symbols[1], symbols[4],
            "Second and fifth 'y' should be the same symbol"
        );
        assert_ne!(
            symbols[0], symbols[1],
            "Different strings should have different symbols"
        );
    }

    #[test]
    fn send_sync_bounds_work() {
        // This test ensures InternPool is Send + Sync
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<InternPool>();
    }

    #[test]
    fn pool_length_tracking() {
        let pool = InternPool::default();
        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);

        pool.intern("first");
        assert_eq!(pool.len(), 1);

        pool.intern("second");
        assert_eq!(pool.len(), 2);

        // Interning the same string shouldn't increase count
        pool.intern("first");
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn pool_cloning() {
        let pool1 = InternPool::default();
        let pool2 = pool1.clone();

        let sym1 = pool1.intern("shared");
        let sym2 = pool2.intern("shared");

        // Both should refer to the same interned string
        assert_eq!(sym1, sym2);
        assert_eq!(pool1.len(), 1);
        assert_eq!(pool2.len(), 1);
    }

    #[test]
    fn parallel_interning_many_strings() {
        let pool = InternPool::default();

        // Intern 1000 strings in parallel
        let symbols: Vec<_> = (0..1000)
            .into_par_iter()
            .map(|i| pool.intern(format!("string_{}", i).as_str()))
            .collect();

        // Verify all were interned
        assert_eq!(symbols.len(), 1000);
        assert_eq!(pool.len(), 1000);

        // Verify each resolves correctly
        for (i, sym) in symbols.iter().enumerate() {
            let resolved = pool.resolve_owned(*sym).expect("should resolve");
            assert_eq!(resolved, format!("string_{}", i));
        }
    }

    #[test]
    fn parallel_interning_with_duplicates() {
        let pool = InternPool::default();

        // Intern strings with many duplicates in parallel
        let base_strings = ["alpha", "beta", "gamma", "delta", "epsilon"];
        let symbols: Vec<_> = (0..500)
            .into_par_iter()
            .map(|i| {
                let s = &base_strings[i % base_strings.len()];
                pool.intern(*s)
            })
            .collect();

        // Should only have 5 unique strings
        assert_eq!(pool.len(), 5);
        assert_eq!(symbols.len(), 500);

        // Verify all symbols resolve correctly
        for sym in symbols.iter() {
            let resolved = pool.resolve_owned(*sym);
            assert!(resolved.is_some());
        }
    }

    #[test]
    fn parallel_batch_interning() {
        let pool = InternPool::default();

        let batches: Vec<Vec<&str>> = (0..10)
            .map(|_batch_idx| {
                (0..100)
                    .map(|i| if i % 2 == 0 { "even" } else { "odd" })
                    .collect()
            })
            .collect();

        // Intern batches in parallel
        let all_symbols: Vec<_> = batches
            .into_par_iter()
            .flat_map(|batch| pool.intern_batch(batch))
            .collect();

        // Should have 1000 symbols total but only 2 unique strings
        assert_eq!(all_symbols.len(), 1000);
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn parallel_mixed_operations() {
        let pool = InternPool::default();

        // Perform mixed operations in parallel
        (0..100).into_par_iter().for_each(|i| {
            let s = format!("item_{}", i % 10);
            let sym = pool.intern(s.as_str());
            let resolved = pool.resolve_owned(sym);
            assert!(resolved.is_some());

            // Use with_resolved as well
            let len = pool.with_resolved(sym, |s| s.len());
            assert!(len.is_some());
        });

        // Should have exactly 10 unique strings
        assert_eq!(pool.len(), 10);
    }

    #[test]
    fn parallel_interning_high_contention() {
        let pool = InternPool::default();

        // Very high contention: all threads intern the same string repeatedly
        (0..1000).into_par_iter().for_each(|_| {
            let _ = pool.intern("hotspot");
        });

        // Should still only have 1 string
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn parallel_clone_and_intern() {
        let pool_original = InternPool::default();

        // Create multiple clones and intern in parallel
        (0..100).into_par_iter().for_each(|i| {
            let pool = pool_original.clone();
            let s = format!("cloned_{}", i % 5);
            let sym = pool.intern(s.as_str());
            let resolved = pool.resolve_owned(sym);
            assert!(resolved.is_some());
        });

        // All clones share the same inner pool
        assert_eq!(pool_original.len(), 5);
    }

    #[test]
    fn intern_pool_inner_direct_usage() {
        let inner = InternPoolInner::new();

        let sym1 = inner.intern("direct");
        let sym2 = inner.intern("direct");
        assert_eq!(sym1, sym2);

        let resolved = inner.resolve_owned(sym1).expect("should resolve");
        assert_eq!(resolved, "direct");
    }
}
