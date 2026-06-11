//! Shared identifier types and counters.

use std::fmt;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

static BLOCK_ID_COUNTER: AtomicU32 = AtomicU32::new(1);
static HIR_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);
static NEXT_SYMBOL_ID: AtomicUsize = AtomicUsize::new(0);
static NEXT_SCOPE_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash, Default, PartialOrd, Ord)]
pub struct BlockId(pub u32);

impl BlockId {
    pub const ROOT_PARENT: Self = Self(u32::MAX);

    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub fn allocate() -> Self {
        Self(BLOCK_ID_COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    pub fn next() -> Self {
        Self(BLOCK_ID_COUNTER.load(Ordering::Relaxed))
    }

    pub const fn as_u32(self) -> u32 {
        self.0
    }

    pub const fn is_root_parent(self) -> bool {
        self.0 == u32::MAX
    }
}

impl fmt::Display for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub fn reset_block_id_counter() {
    BLOCK_ID_COUNTER.store(1, Ordering::Relaxed);
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash, Default, PartialOrd, Ord)]
pub struct HirId(pub usize);

impl HirId {
    pub fn new() -> Self {
        next_hir_id()
    }

    pub fn next() -> Self {
        Self(HIR_ID_COUNTER.load(Ordering::Relaxed))
    }
}

impl fmt::Display for HirId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub fn next_hir_id() -> HirId {
    HirId(HIR_ID_COUNTER.fetch_add(1, Ordering::Relaxed))
}

pub fn reset_hir_id_counter() {
    HIR_ID_COUNTER.store(0, Ordering::Relaxed);
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default, PartialOrd, Ord)]
pub struct SymId(pub usize);

pub type SymbolId = SymId;

impl fmt::Display for SymId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub(crate) fn next_symbol_id() -> SymId {
    SymId(NEXT_SYMBOL_ID.fetch_add(1, Ordering::Relaxed))
}

pub fn reset_symbol_id_counter() {
    NEXT_SYMBOL_ID.store(0, Ordering::Relaxed);
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default, PartialOrd, Ord)]
pub struct ScopeId(pub usize);

impl fmt::Display for ScopeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub(crate) fn next_scope_id() -> ScopeId {
    ScopeId(NEXT_SCOPE_ID.fetch_add(1, Ordering::Relaxed))
}

pub fn reset_scope_id_counter() {
    NEXT_SCOPE_ID.store(0, Ordering::Relaxed);
}
