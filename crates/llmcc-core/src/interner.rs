use std::sync::RwLock;

use string_interner::backend::DefaultBackend;
use string_interner::symbol::DefaultSymbol;
use string_interner::StringInterner;

/// Interned string symbol backed by a `StringInterner`.
pub type InternedStr = DefaultSymbol;

/// Shared string interner used across the llmcc core.
#[derive(Debug)]
pub struct InternPool {
    inner: RwLock<StringInterner<DefaultBackend>>,
}

impl Default for InternPool {
    fn default() -> Self {
        Self {
            inner: RwLock::new(StringInterner::new()),
        }
    }
}

impl InternPool {
    /// Intern the provided string slice and return its symbol.
    pub fn intern<S>(&self, value: S) -> InternedStr
    where
        S: AsRef<str>,
    {
        self.inner.write().unwrap().get_or_intern(value.as_ref())
    }

    /// Resolve an interned symbol back into an owned string.
    ///
    /// Clones the underlying string from the interner to avoid lifetime issues.
    pub fn resolve_owned(&self, symbol: InternedStr) -> Option<String> {
        self.inner
            .read()
            .unwrap()
            .resolve(symbol)
            .map(|s| s.to_owned())
    }

    /// Resolve an interned symbol and apply a closure while the borrow is active.
    pub fn with_resolved<R, F>(&self, symbol: InternedStr, f: F) -> Option<R>
    where
        F: FnOnce(&str) -> R,
    {
        self.inner.read().unwrap().resolve(symbol).map(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
