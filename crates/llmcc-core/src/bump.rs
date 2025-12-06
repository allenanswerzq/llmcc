// Thread 1's Bump     Thread 2's Bump     Thread 3's Bump
// ┌─────────────┐    ┌─────────────┐    ┌─────────────┐
// │ Memory Pool │    │ Memory Pool │    │ Memory Pool │
// │  (Arena A)  │    │  (Arena B)  │    │  (Arena C)  │
// └─────────────┘    └─────────────┘    └─────────────┘
//         │                  │                  │
//         └──────────────────┼──────────────────┘
//                            │
//                   Shared Vec<&T>
//                   (protected by RwLock)
#[macro_export]
macro_rules! declare_arena {
    ($arena_name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        use std::ops::{Deref, DerefMut};
        use std::sync::Arc;
        use bumpalo_herd::Herd;
        use parking_lot::RwLock;

        /// The actual data container: thread-safe, allocation-aware, not cloneable directly.
        #[derive(Debug)]
        pub struct ArenaInner<'a> {
            pub herd: Herd,
            $( pub $field: RwLock<Vec<&'a $ty>>, )*
        }

        impl<'a> ArenaInner<'a> {
            /// Create a fresh ArenaInner.
            pub fn new() -> Self {
                Self {
                    herd: Herd::new(),
                    $( $field: RwLock::new(Vec::new()), )*
                }
            }

            /// Create ArenaInner with pre-allocated capacity for each field.
            #[allow(dead_code)]
            pub fn new_with_capacity(cap: usize) -> Self {
                Self {
                    herd: Herd::new(),
                    $( $field: RwLock::new(Vec::with_capacity(cap)), )*
                }
            }

            /// Clear all allocations and reset memory.
            #[allow(dead_code)]
            pub fn reset(&mut self) {
                $( self.$field.get_mut().clear(); )*
                self.herd.reset();
            }

            /// Thread-safe allocation; no mutable reference needed.
            #[inline]
            pub fn alloc<T: ArenaInsert<'a>>(&'a self, value: T) -> &'a T {
                value.insert_into(self)
            }

            // ----- Auto-generated getters -----
            $(
                paste::paste! {
                    /// Get read access to all items (returns a guard, no clone).
                    #[inline]
                    pub fn [<$field>](&self) -> parking_lot::RwLockReadGuard<'_, Vec<&'a $ty>> {
                        self.$field.read()
                    }

                    /// Get count without cloning the Vec.
                    #[inline]
                    #[allow(dead_code)]
                    pub fn [<len_ $field>](&self) -> usize {
                        self.$field.read().len()
                    }

                    /// Sort items in-place by a key function.
                    #[inline]
                    #[allow(dead_code)]
                    pub fn [<$field _sort_by>]<K: Ord>(&self, f: impl FnMut(&&'a $ty) -> K) {
                        self.$field.write().sort_by_key(f);
                    }
                }
            )*
        }

        impl<'a> Default for ArenaInner<'a> {
            fn default() -> Self {
                Self::new()
            }
        }

        /// Shared wrapper around `ArenaInner`, safely cloneable across threads.
        #[derive(Clone, Debug)]
        pub struct $arena_name<'a> {
            inner: Arc<ArenaInner<'a>>,
        }

        impl<'a> $arena_name<'a> {
            /// Create a new shared Arena.
            pub fn new() -> Self {
                Self { inner: Arc::new(ArenaInner::new()) }
            }

            /// Get the internal Arc.
            #[inline]
            #[allow(dead_code)]
            pub fn inner_arc(&self) -> &Arc<ArenaInner<'a>> {
                &self.inner
            }
        }

        impl<'a> Default for $arena_name<'a> {
            fn default() -> Self {
                Self::new()
            }
        }

        // ---- Deref to ArenaInner ----
        impl<'a> Deref for $arena_name<'a> {
            type Target = ArenaInner<'a>;
            #[inline]
            fn deref(&self) -> &Self::Target {
                &self.inner
            }
        }

        impl<'a> DerefMut for $arena_name<'a> {
            #[inline]
            fn deref_mut(&mut self) -> &mut Self::Target {
                // NOTE: This panics if multiple Arcs exist. Use only when uniquely owned.
                Arc::get_mut(&mut self.inner)
                    .expect("Cannot get mutable reference to ArenaInner: multiple Arc references exist")
            }
        }

        /// Trait implemented by all types that can be allocated in the arena.
        pub trait ArenaInsert<'a>: Sized {
            fn insert_into(self, arena: &'a ArenaInner<'a>) -> &'a Self;
        }

        // ---- Auto-generate ArenaInsert impls for each field ----
        $(
            impl<'a> ArenaInsert<'a> for $ty {
                #[inline]
                fn insert_into(self, arena: &'a ArenaInner<'a>) -> &'a Self {
                    let member = arena.herd.get();
                    let r = member.alloc(self);
                    arena.$field.write().push(r);
                    r
                }
            }
        )*
    };
}
