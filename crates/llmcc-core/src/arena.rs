#[macro_export]
macro_rules! declare_arena {
    // Main entry point with optional vec fields: declare_arena!([foo: Foo, bar: Bar] @vec [baz: Baz])
    ([$($arena_name:ident : $arena_ty:ty),* $(,)?] @vec [$($vec_name:ident : $vec_ty:ty),* $(,)?]) => {
        $crate::declare_arena! { @impl
            arena_fields: [$($arena_name : $arena_ty),*]
            vec_fields: [$($vec_name : $vec_ty),*]
        }
    };

    // Default: all arena, no vec (backward compatible)
    ([$($arena_name:ident : $arena_ty:ty),* $(,)?]) => {
        $crate::declare_arena! { @impl
            arena_fields: [$($arena_name : $arena_ty),*]
            vec_fields: []
        }
    };

    // Implementation
    (@impl arena_fields: [$($arena_name:ident : $arena_ty:ty),*] vec_fields: [$($vec_name:ident : $vec_ty:ty),*]) => {
        #[derive(Default)]
        pub struct Arena<'tcx> {
            $( pub $arena_name : typed_arena::Arena<$arena_ty>, )*
            $( pub $vec_name : parking_lot::RwLock<Vec<$vec_ty>>, )*
            _marker: std::marker::PhantomData<&'tcx ()>,
        }

        impl<'tcx> std::fmt::Debug for Arena<'tcx> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct("Arena").finish()
            }
        }

        pub trait ArenaAllocatable<'tcx>: Sized {
            fn allocate_on(self, arena: &'tcx Arena<'tcx>) -> &'tcx Self;
        }

        #[allow(clippy::mut_from_ref)]
        pub trait ArenaAllocatableMut<'tcx>: ArenaAllocatable<'tcx> {
            fn allocate_on_mut(self, arena: &'tcx Arena<'tcx>) -> &'tcx mut Self;
        }

        // Allocatable implementations for typed_arena fields
        $(
            impl<'tcx> ArenaAllocatable<'tcx> for $arena_ty {
                #[inline]
                fn allocate_on(self, arena: &'tcx Arena<'tcx>) -> &'tcx Self {
                    arena.$arena_name.alloc(self)
                }
            }

            impl<'tcx> ArenaAllocatableMut<'tcx> for $arena_ty {
                #[inline]
                fn allocate_on_mut(self, arena: &'tcx Arena<'tcx>) -> &'tcx mut Self {
                    arena.$arena_name.alloc(self)
                }
            }
        )*

        // Allocatable implementations for vec fields
        $(
            impl<'tcx> ArenaAllocatable<'tcx> for $vec_ty {
                #[inline]
                fn allocate_on(self, arena: &'tcx Arena<'tcx>) -> &'tcx Self {
                    let mut vec = arena.$vec_name.write();
                    vec.push(self);
                    unsafe { &*(vec.last().unwrap() as *const _) }
                }
            }

            impl<'tcx> ArenaAllocatableMut<'tcx> for $vec_ty {
                #[inline]
                fn allocate_on_mut(self, arena: &'tcx Arena<'tcx>) -> &'tcx mut Self {
                    let mut vec = arena.$vec_name.write();
                    vec.push(self);
                    unsafe { &mut *(vec.last_mut().unwrap() as *mut _) }
                }
            }
        )*

        impl<'tcx> Arena<'tcx> {
            #[inline]
            pub fn alloc<T: ArenaAllocatable<'tcx>>(&'tcx self, value: T) -> &'tcx T {
                value.allocate_on(self)
            }

            #[inline]
            pub fn alloc_mut<T: ArenaAllocatableMut<'tcx>>(&'tcx self, value: T) -> &'tcx mut T {
                value.allocate_on_mut(self)
            }

            // Iterator methods for vec fields (immutable iteration only)
            $(
                paste::paste! {
                    /// Iterate over all allocated values in the `$vec_name` vector.
                    /// Items are yielded in the order they were allocated.
                    pub fn [<iter_ $vec_name>](&self) -> impl Iterator<Item = &$vec_ty> {
                        let guard = self.$vec_name.read();
                        // SAFETY: We extend the lifetime from the guard to 'tcx
                        // This is safe because the Arena lives for 'tcx and won't be destroyed
                        let iter: std::slice::Iter<'tcx, $vec_ty> = unsafe { std::mem::transmute(guard.iter()) };
                        std::mem::forget(guard); // Leak the guard to keep the lock held
                        iter
                    }
                }
            )*
        }

        unsafe impl<'tcx> Sync for Arena<'tcx> {}
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    pub struct Foo(i32);

    #[derive(Debug, PartialEq)]
    pub struct Baz(f64);

    declare_arena!([
        foos: Foo
    ] @vec [
        bazzes: Baz
    ]);

    #[test]
    fn alloc_single_values() {
        let arena = Arena::default();

        let f = arena.alloc(Foo(42));
        let b = arena.alloc(Baz(3.14));

        assert_eq!(f, &Foo(42));
        assert_eq!(b, &Baz(3.14));
    }

    #[test]
    fn alloc_multiple_values() {
        let arena = Arena::default();

        let f1 = arena.alloc(Foo(1));
        let b1 = arena.alloc(Baz(1.0));
        let f2 = arena.alloc(Foo(2));
        let b2 = arena.alloc(Baz(2.0));

        assert_eq!(f1, &Foo(1));
        assert_eq!(b1, &Baz(1.0));
        assert_eq!(f2, &Foo(2));
        assert_eq!(b2, &Baz(2.0));
    }

    #[test]
    fn alloc_mut_arena_field() {
        let arena = Arena::default();

        let foo = arena.alloc_mut(Foo(1));
        foo.0 = 100;

        assert_eq!(foo, &Foo(100));
    }

    #[test]
    fn alloc_mut_vec_field() {
        let arena = Arena::default();

        let baz = arena.alloc_mut(Baz(1.5));
        baz.0 = 99.9;

        assert_eq!(baz, &Baz(99.9));
    }

    #[test]
    fn iter_vec_fields() {
        let arena = Arena::default();

        arena.alloc(Baz(1.0));
        arena.alloc(Baz(2.0));
        arena.alloc(Baz(3.0));

        let results: Vec<_> = arena.iter_bazzes().map(|b| b.0).collect();
        assert_eq!(results, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn iter_both_types() {
        let arena = Arena::default();

        arena.alloc(Foo(1));
        arena.alloc(Baz(1.5));
        arena.alloc(Foo(2));
        arena.alloc(Baz(2.5));

        let baz_results: Vec<_> = arena.iter_bazzes().map(|b| b.0).collect();
        assert_eq!(baz_results, vec![1.5, 2.5]);
    }

    #[test]
    fn vec_field_preserves_order() {
        let arena = Arena::default();

        arena.alloc(Baz(1.0));
        arena.alloc(Baz(2.0));
        arena.alloc(Baz(3.0));
        arena.alloc(Baz(4.0));

        let bazzes: Vec<_> = arena.iter_bazzes().map(|b| b.0).collect();
        assert_eq!(bazzes, vec![1.0, 2.0, 3.0, 4.0]);
    }
}
