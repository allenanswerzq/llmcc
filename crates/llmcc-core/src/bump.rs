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
//
// SAFETY: The raw pointers point to data allocated in the bump arena.
// The arena outlives all usage of the pointers.

#[macro_export]
macro_rules! declare_arena {
    ($arena_name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        /// The actual data container: thread-safe, allocation-aware.
        /// Uses 'static lifetime for storage since we use raw pointers internally.
        pub struct ArenaInner {
            pub herd: bumpalo_herd::Herd,
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
                    herd: bumpalo_herd::Herd::new(),
                    // Use 256 shards to reduce contention at high thread counts
                    $( $field: dashmap::DashMap::new(), )*
                }
            }

            /// Create ArenaInner with pre-allocated capacity for each field.
            #[allow(dead_code)]
            pub fn new_with_capacity(cap: usize) -> Self {
                Self {
                    herd: bumpalo_herd::Herd::new(),
                    $( $field: dashmap::DashMap::with_capacity(cap), )*
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

            /// Allocate a value in the thread-local bump allocator.
            /// Uses thread_local to cache the Member per thread, avoiding mutex contention.
            #[inline]
            pub fn alloc_in_herd<T>(&self, value: T) -> &T {
                // PERFORMANCE CRITICAL: bumpalo_herd::Herd::get() contains a mutex.
                // Calling it millions of times causes severe lock contention at high thread counts.
                // We use thread_local to cache the Member per-thread.
                //
                // SAFETY: The Member borrows from Herd which lives in ArenaInner (Arc).
                // By using 'static in the thread_local, we're telling Rust we'll manage
                // the lifetime ourselves. The ArenaInner (and thus Herd) outlives all
                // parallel processing phases where this is called.
                thread_local! {
                    static CACHED_MEMBER: std::cell::RefCell<Option<bumpalo_herd::Member<'static>>> =
                        const { std::cell::RefCell::new(None) };
                }

                CACHED_MEMBER.with(|cell| {
                    let mut borrow = cell.borrow_mut();
                    let member = borrow.get_or_insert_with(|| {
                        // Get member once per thread and cache it
                        // SAFETY: We transmute the lifetime to 'static for storage in thread_local.
                        // This is safe because:
                        // 1. The Herd lives in Arc<ArenaInner> which outlives parallel processing
                        // 2. Member is only accessed from this thread
                        // 3. The actual allocation lifetime is 'a from the outer function
                        let member = self.herd.get();
                        unsafe { std::mem::transmute::<bumpalo_herd::Member<'_>, bumpalo_herd::Member<'static>>(member) }
                    });
                    // SAFETY: The returned reference has lifetime 'a, bound to ArenaInner
                    member.alloc(value)
                })
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
            impl<'a> ArenaInsertWithId<'a> for $ty {
                #[inline]
                fn insert_with_id(self, arena: &'a ArenaInner, id: usize) -> &'a Self {
                    // PERFORMANCE: Use alloc_with_member to avoid repeated herd.get() calls
                    // The bumpalo_herd::Member is cached per-thread
                    let r: &'a Self = arena.alloc_in_herd(self);
                    // Store raw pointer as *const () to avoid lifetime issues
                    arena.$field.insert(id, r as *const Self as *const ());
                    r
                }
            }

            impl<'a> ArenaInsert<'a> for $ty {
                #[inline]
                fn insert_into(self, arena: &'a ArenaInner) -> &'a Self {
                    // Just allocate, no DashMap insert
                    arena.alloc_in_herd(self)
                }
            }
        )*
    };
}
