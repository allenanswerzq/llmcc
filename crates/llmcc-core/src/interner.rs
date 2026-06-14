//! Thread-safe string interning for compiler symbols.
//!
//! The pool stores each distinct string once and returns compact symbols for
//! names used throughout HIR, scopes, and symbol tables. Cloning an
//! [`InternPool`] shares the same backing table.

use parking_lot::RwLock;
use std::sync::Arc;

use string_interner::StringInterner;
use string_interner::backend::DefaultBackend;
use string_interner::symbol::DefaultSymbol;

/// Opaque symbol for an interned string.
///
/// A symbol is meaningful only with the [`InternPool`] that produced it. The
/// compiler normally uses one shared pool per [`CompileCtxt`](crate::context::CompileCtxt).
pub type InternedStr = DefaultSymbol;

type RawInterner = StringInterner<DefaultBackend>;

#[derive(Debug)]
struct InternPoolInner {
    interner: RwLock<RawInterner>,
}

impl InternPoolInner {
    fn new() -> Self {
        Self {
            interner: RwLock::new(StringInterner::new()),
        }
    }

    #[inline]
    fn intern<S>(&self, value: S) -> InternedStr
    where
        S: AsRef<str>,
    {
        let value = value.as_ref();

        if let Some(symbol) = self.try_get(value) {
            return symbol;
        }

        self.interner.write().get_or_intern(value)
    }

    fn intern_many<S>(&self, values: impl IntoIterator<Item = S>) -> Vec<InternedStr>
    where
        S: AsRef<str>,
    {
        values.into_iter().map(|value| self.intern(value)).collect()
    }

    fn try_get<S>(&self, value: S) -> Option<InternedStr>
    where
        S: AsRef<str>,
    {
        self.interner.read().get(value.as_ref())
    }

    fn try_resolve(&self, symbol: InternedStr) -> Option<String> {
        self.with_str(symbol, |value| value.to_owned())
    }

    fn with_str<R, F>(&self, symbol: InternedStr, f: F) -> Option<R>
    where
        F: FnOnce(&str) -> R,
    {
        self.interner.read().resolve(symbol).map(f)
    }

    fn len(&self) -> usize {
        self.interner.read().len()
    }
}

/// Shared, thread-safe string interner.
///
/// `InternPool` is cheap to clone; clones share the same table. Use
/// [`intern`](Self::intern) to create or fetch a symbol, and
/// [`try_resolve`](Self::try_resolve) to clone the string for display or output.
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

    /// Return the symbol for `value`, inserting it when needed.
    pub fn intern<S>(&self, value: S) -> InternedStr
    where
        S: AsRef<str>,
    {
        self.inner.intern(value)
    }

    /// Intern each string from `values` and return symbols in input order.
    pub fn intern_many<S>(&self, values: impl IntoIterator<Item = S>) -> Vec<InternedStr>
    where
        S: AsRef<str>,
    {
        self.inner.intern_many(values)
    }

    /// Intern each string from `values` and return symbols in input order.
    ///
    /// Alias for [`intern_many`](Self::intern_many).
    pub fn intern_batch<S>(&self, values: impl IntoIterator<Item = S>) -> Vec<InternedStr>
    where
        S: AsRef<str>,
    {
        self.intern_many(values)
    }

    /// Return the existing symbol for `value` without inserting it.
    pub fn try_get<S>(&self, value: S) -> Option<InternedStr>
    where
        S: AsRef<str>,
    {
        self.inner.try_get(value)
    }

    /// Resolve `symbol` into an owned string.
    pub fn try_resolve(&self, symbol: InternedStr) -> Option<String> {
        self.inner.try_resolve(symbol)
    }

    /// Resolve `symbol` into an owned string.
    ///
    /// Kept for callers that prefer the explicit ownership in the name. New
    /// code should usually use [`try_resolve`](Self::try_resolve).
    pub fn resolve_owned(&self, symbol: InternedStr) -> Option<String> {
        self.try_resolve(symbol)
    }

    /// Borrow the interned string for `symbol` and apply `f` while it is live.
    ///
    /// The closure runs while the pool's read lock is held. Keep it small and
    /// do not call methods that may intern new strings on the same pool from it.
    pub fn with_str<R, F>(&self, symbol: InternedStr, f: F) -> Option<R>
    where
        F: FnOnce(&str) -> R,
    {
        self.inner.with_str(symbol, f)
    }

    /// Borrow the interned string for `symbol` and apply `f` while it is live.
    ///
    /// Alias for [`with_str`](Self::with_str).
    pub fn with_resolved<R, F>(&self, symbol: InternedStr, f: F) -> Option<R>
    where
        F: FnOnce(&str) -> R,
    {
        self.with_str(symbol, f)
    }

    /// Return the current number of unique interned strings.
    ///
    /// In concurrent use this is only a moment-in-time diagnostic snapshot.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Return `true` when no strings have been interned.
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
    fn try_get_only_reports_existing_symbols() {
        let pool = InternPool::default();

        assert_eq!(pool.try_get("missing"), None);

        let symbol = pool.intern("present");
        assert_eq!(pool.try_get("present"), Some(symbol));
    }

    #[test]
    fn try_resolve_recovers_string() {
        let pool = InternPool::default();
        let sym = pool.intern("bar");
        let resolved = pool
            .try_resolve(sym)
            .expect("symbol should resolve to a string");
        assert_eq!(resolved, "bar");
    }

    #[test]
    fn resolve_owned_remains_supported() {
        let pool = InternPool::default();
        let sym = pool.intern("owned");
        assert_eq!(pool.resolve_owned(sym).as_deref(), Some("owned"));
    }

    #[test]
    fn with_str_provides_borrowed_str() {
        let pool = InternPool::default();
        let sym = pool.intern("baz");
        let length = pool
            .with_str(sym, |s| s.len())
            .expect("symbol should resolve to a closure result");
        assert_eq!(length, 3);
    }

    #[test]
    fn with_resolved_remains_supported() {
        let pool = InternPool::default();
        let sym = pool.intern("borrowed");
        assert_eq!(pool.with_resolved(sym, str::len), Some(8));
    }

    #[test]
    fn intern_many_interns_multiple_strings() {
        let pool = InternPool::default();
        let strings = vec!["apple", "banana", "cherry"];
        let symbols = pool.intern_many(strings.clone());

        assert_eq!(symbols.len(), 3);

        for (i, sym) in symbols.iter().enumerate() {
            let resolved = pool.try_resolve(*sym).expect("symbol should resolve");
            assert_eq!(resolved, strings[i]);
        }
    }

    #[test]
    fn intern_many_with_duplicates() {
        let pool = InternPool::default();
        let strings = vec!["x", "y", "x", "z", "y"];
        let symbols = pool.intern_many(strings);

        assert_eq!(pool.intern_batch(["x"]), vec![symbols[0]]);

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

        pool.intern("first");
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn pool_cloning() {
        let pool1 = InternPool::default();
        let pool2 = pool1.clone();

        let sym1 = pool1.intern("shared");
        let sym2 = pool2.intern("shared");

        assert_eq!(sym1, sym2);
        assert_eq!(pool1.len(), 1);
        assert_eq!(pool2.len(), 1);
    }

    #[test]
    fn parallel_interning_many_strings() {
        let pool = InternPool::default();

        let symbols: Vec<_> = (0..1000)
            .into_par_iter()
            .map(|i| pool.intern(format!("string_{i}")))
            .collect();

        assert_eq!(symbols.len(), 1000);
        assert_eq!(pool.len(), 1000);

        for (i, sym) in symbols.iter().enumerate() {
            let resolved = pool.try_resolve(*sym).expect("should resolve");
            assert_eq!(resolved, format!("string_{i}"));
        }
    }

    #[test]
    fn parallel_interning_with_duplicates() {
        let pool = InternPool::default();

        let base_strings = ["alpha", "beta", "gamma", "delta", "epsilon"];
        let symbols: Vec<_> = (0..500)
            .into_par_iter()
            .map(|i| {
                let s = &base_strings[i % base_strings.len()];
                pool.intern(*s)
            })
            .collect();

        assert_eq!(pool.len(), 5);
        assert_eq!(symbols.len(), 500);

        for sym in symbols.iter() {
            let resolved = pool.try_resolve(*sym);
            assert!(resolved.is_some());
        }
    }

    #[test]
    fn parallel_many_interning() {
        let pool = InternPool::default();

        let batches: Vec<Vec<&str>> = (0..10)
            .map(|_batch_idx| {
                (0..100)
                    .map(|i| if i % 2 == 0 { "even" } else { "odd" })
                    .collect()
            })
            .collect();

        let all_symbols: Vec<_> = batches
            .into_par_iter()
            .flat_map(|batch| pool.intern_many(batch))
            .collect();

        assert_eq!(all_symbols.len(), 1000);
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn parallel_mixed_operations() {
        let pool = InternPool::default();

        (0..100).into_par_iter().for_each(|i| {
            let s = format!("item_{}", i % 10);
            let sym = pool.intern(s.as_str());
            let resolved = pool.try_resolve(sym);
            assert!(resolved.is_some());

            let len = pool.with_str(sym, str::len);
            assert!(len.is_some());
        });

        assert_eq!(pool.len(), 10);
    }

    #[test]
    fn parallel_interning_high_contention() {
        let pool = InternPool::default();

        (0..1000).into_par_iter().for_each(|_| {
            let _ = pool.intern("hotspot");
        });

        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn parallel_clone_and_intern() {
        let pool_original = InternPool::default();

        (0..100).into_par_iter().for_each(|i| {
            let pool = pool_original.clone();
            let s = format!("cloned_{}", i % 5);
            let sym = pool.intern(s.as_str());
            let resolved = pool.try_resolve(sym);
            assert!(resolved.is_some());
        });

        assert_eq!(pool_original.len(), 5);
    }
}
