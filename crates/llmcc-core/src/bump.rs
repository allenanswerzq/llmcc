//! Thread-safe bump allocator utilities.

// Thread 1's Bump     Thread 2's Bump     Thread 3's Bump
// ┌─────────────┐    ┌─────────────┐    ┌─────────────┐
// │ Memory Pool │    │ Memory Pool │    │ Memory Pool │
// │  (Arena A)  │    │  (Arena B)  │    │  (Arena C)  │
// └─────────────┘    └─────────────┘    └─────────────┘
//         │                  │                  │
//         └──────────────────┼──────────────────┘
//                            │
//                   DashMap<Id, *const T>
//                   (concurrent, no global lock)
//
// Arena allocates in thread-local bump (fast, no contention).
// DashMap stores raw pointers for O(1) concurrent lookup by ID.
// No Vec, no RwLock - maximum parallel performance!
#[macro_export]
macro_rules! declare_arena {
    ($arena_name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        /// The actual data container: thread-safe, allocation-aware.
        /// Uses 'static lifetime for storage since we use raw pointers internally.
        pub struct ArenaInner {
            pub herd: llmcc_bumpalo::Herd,
            // DashMap for each type - stores raw pointers for concurrent insert & lookup
            $( pub $field: dashmap::DashMap<usize, *const ()>, )*
        }

        // SAFETY: The raw pointers in DashMap point to bump-allocated data that is:
        // 1. Immutable after allocation (never modified)
        // 2. Lives as long as the Arena (bump allocator owns the memory)
        // 3. Thread-safe to read from multiple threads
        unsafe impl Send for ArenaInner {}
        unsafe impl Sync for ArenaInner {}

        impl std::fmt::Debug for ArenaInner {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct("ArenaInner")
                    $( .field(stringify!($field), &self.$field.len()) )*
                    .finish()
            }
        }

        impl ArenaInner {
            /// Create a fresh ArenaInner.
            pub fn new() -> Self {
                Self {
                    herd: llmcc_bumpalo::Herd::new(),
                    // Use 256 shards to reduce contention at high thread counts
                    $( $field: dashmap::DashMap::with_hasher_and_shard_amount(std::hash::RandomState::new(), 256), )*
                }
            }

            /// Create ArenaInner with pre-allocated capacity for each field.
            #[allow(dead_code)]
            pub fn new_with_capacity(cap: usize) -> Self {
                Self {
                    herd: llmcc_bumpalo::Herd::new(),
                    $( $field: dashmap::DashMap::with_capacity_and_hasher_and_shard_amount(cap, std::hash::RandomState::new(), 256), )*
                }
            }

            /// Clear all allocations and reset memory.
            #[allow(dead_code)]
            pub fn reset(&mut self) {
                $( self.$field.clear(); )*
                self.herd.reset();
            }

            /// Thread-safe allocation with ID for lookup.
            /// Allocates in bump arena and inserts into DashMap.
            #[inline]
            pub fn alloc_with_id<'a, T: ArenaInsertWithId<'a>>(&'a self, id: usize, value: T) -> &'a T {
                value.insert_with_id(self, id)
            }

            /// Thread-safe allocation without ID tracking.
            /// Just allocates in bump arena, no DashMap insert.
            #[inline]
            pub fn alloc<'a, T: ArenaInsert<'a>>(&'a self, value: T) -> &'a T {
                value.insert_into(self)
            }

            /// Allocate a value in this thread's herd-owned bump allocator.
            #[inline]
            pub fn alloc_in_herd<T>(&self, value: T) -> &T {
                self.herd.alloc(value)
            }

            /// Allocate a string in this thread's herd-owned bump allocator.
            /// Returns a reference to the arena-allocated string, avoiding heap allocation.
            #[inline]
            pub fn alloc_str(&self, src: &str) -> &str {
                self.herd.alloc_str(src)
            }

            // ----- Auto-generated getters -----
            $(
                paste::paste! {
                    /// Get item by ID from DashMap (O(1) concurrent lookup).
                    /// SAFETY: The pointer was allocated from the bump arena
                    /// and is valid for the arena's lifetime.
                    #[inline]
                    #[allow(clippy::needless_lifetimes)]
                    pub fn [<get_ $field>]<'a>(&'a self, id: usize) -> Option<&'a $ty> {
                        self.$field.get(&id).map(|r| {
                            // SAFETY: Pointer is valid for 'a lifetime
                            // Cast from *const () to *const $ty
                            unsafe { &*(*r.value() as *const $ty) }
                        })
                    }

                    /// Get count of items.
                    #[inline]
                    #[allow(dead_code)]
                    pub fn [<len_ $field>](&self) -> usize {
                        self.$field.len()
                    }

                    /// Iterate over all items (for compatibility).
                    /// SAFETY: All pointers are valid for the arena's lifetime.
                    #[inline]
                    #[allow(dead_code)]
                    pub fn [<iter_ $field>]<'a>(&'a self) -> impl Iterator<Item = &'a $ty> + 'a {
                        self.$field.iter().map(|r| {
                            // SAFETY: Pointer is valid for 'a lifetime
                            unsafe { &*(*r.value() as *const $ty) }
                        })
                    }
                }
            )*
        }

        impl Default for ArenaInner {
            fn default() -> Self {
                Self::new()
            }
        }

        /// Shared wrapper around `ArenaInner`, safely cloneable across threads.
        #[derive(Clone, Debug)]
        pub struct $arena_name<'a> {
            inner: std::sync::Arc<ArenaInner>,
            _marker: std::marker::PhantomData<&'a ()>,
        }

        impl<'a> $arena_name<'a> {
            /// Create a new shared Arena.
            pub fn new() -> Self {
                Self {
                    inner: std::sync::Arc::new(ArenaInner::new()),
                    _marker: std::marker::PhantomData,
                }
            }

            /// Get the internal Arc.
            #[inline]
            #[allow(dead_code)]
            pub fn inner_arc(&self) -> &std::sync::Arc<ArenaInner> {
                &self.inner
            }
        }

        impl<'a> Default for $arena_name<'a> {
            fn default() -> Self {
                Self::new()
            }
        }

        // ---- Deref to ArenaInner ----
        impl<'a> std::ops::Deref for $arena_name<'a> {
            type Target = ArenaInner;
            #[inline]
            fn deref(&self) -> &Self::Target {
                &self.inner
            }
        }

        impl<'a> std::ops::DerefMut for $arena_name<'a> {
            #[inline]
            fn deref_mut(&mut self) -> &mut Self::Target {
                std::sync::Arc::get_mut(&mut self.inner)
                    .expect("Cannot get mutable reference to ArenaInner: multiple Arc references exist")
            }
        }

        /// Trait for types that can be allocated with an ID for lookup.
        pub trait ArenaInsertWithId<'a>: Sized {
            fn insert_with_id(self, arena: &'a ArenaInner, id: usize) -> &'a Self;
        }

        /// Trait for types that can be allocated without ID tracking.
        pub trait ArenaInsert<'a>: Sized {
            fn insert_into(self, arena: &'a ArenaInner) -> &'a Self;
        }

        // ---- Auto-generate impls for each field ----
        $(
            impl<'a> ArenaInsertWithId<'a> for $ty
            where
                $ty: Send + Sync,
            {
                #[inline]
                fn insert_with_id(self, arena: &'a ArenaInner, id: usize) -> &'a Self {
                    let r: &'a Self = arena.alloc_in_herd(self);
                    let old = arena.$field.insert(id, r as *const Self as *const ());
                    debug_assert!(
                        old.is_none(),
                        "duplicate id {id} inserted into arena field {}",
                        stringify!($field),
                    );
                    r
                }
            }

            impl<'a> ArenaInsert<'a> for $ty
            where
                $ty: Send + Sync,
            {
                #[inline]
                fn insert_into(self, arena: &'a ArenaInner) -> &'a Self {
                    arena.alloc_in_herd(self)
                }
            }
        )*
    };
}

#[cfg(test)]
mod tests {
    use std::ptr;
    use std::time::{Duration, Instant};

    #[derive(Debug, PartialEq, Eq)]
    pub(crate) struct TestNode {
        value: usize,
    }

    #[derive(Debug, PartialEq, Eq)]
    pub(crate) struct TestRef<'a> {
        name: &'a str,
    }

    declare_arena!(TestArena {
        node: TestNode,
        named: TestRef<'a>,
    });

    #[test]
    fn allocates_and_looks_up_by_id() {
        let arena = TestArena::new();

        let node = arena.alloc_with_id(7, TestNode { value: 42 });
        let fetched = arena.get_node(7).expect("node should be indexed");

        assert_eq!(fetched.value, 42);
        assert!(ptr::eq(node, fetched));
        assert_eq!(arena.len_node(), 1);
    }

    #[test]
    fn allocates_without_indexing() {
        let arena = TestArena::new();

        let node = arena.alloc(TestNode { value: 11 });

        assert_eq!(node.value, 11);
        assert_eq!(arena.len_node(), 0);
    }

    #[test]
    fn allocates_strings_for_arena_lifetime() {
        let arena = TestArena::new();
        let source = String::from("module::item");

        let stored = arena.alloc_str(&source);
        let named = arena.alloc_with_id(1, TestRef { name: stored });

        drop(source);

        assert_eq!(named.name, "module::item");
        assert_eq!(arena.get_named(1).unwrap().name, "module::item");
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "duplicate id 1 inserted into arena field node")]
    fn duplicate_ids_panic_in_debug_builds() {
        let arena = TestArena::new();

        arena.alloc_with_id(1, TestNode { value: 1 });
        arena.alloc_with_id(1, TestNode { value: 2 });
    }

    #[test]
    fn reset_clears_indexes_and_keeps_arena_reusable() {
        let mut arena = TestArena::new();

        arena.alloc_with_id(1, TestNode { value: 1 });
        assert_eq!(arena.len_node(), 1);

        arena.reset();
        assert_eq!(arena.len_node(), 0);
        assert!(arena.get_node(1).is_none());

        arena.alloc_with_id(2, TestNode { value: 2 });
        assert_eq!(arena.get_node(2).unwrap().value, 2);
    }

    #[test]
    fn supports_concurrent_allocations_and_lookups() {
        let arena: TestArena<'static> = TestArena::new();
        const THREADS: usize = 8;
        const PER_THREAD: usize = 1_000;

        std::thread::scope(|scope| {
            for thread_index in 0..THREADS {
                let arena = arena.clone();
                scope.spawn(move || {
                    for offset in 0..PER_THREAD {
                        let id = thread_index * PER_THREAD + offset;
                        arena.alloc_with_id(id, TestNode { value: id });
                    }
                });
            }
        });

        assert_eq!(arena.len_node(), THREADS * PER_THREAD);
        for id in [0, 999, 1_000, 4_321, 7_999] {
            assert_eq!(arena.get_node(id).unwrap().value, id);
        }
    }

    #[test]
    fn bulk_allocation_performance_smoke_test() {
        let arena = TestArena::new();
        let started = Instant::now();

        for id in 0..50_000 {
            arena.alloc_with_id(id, TestNode { value: id });
        }

        assert_eq!(arena.len_node(), 50_000);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "50k arena allocations took {:?}",
            started.elapsed()
        );
    }
}
