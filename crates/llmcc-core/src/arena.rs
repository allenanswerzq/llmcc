#[macro_export]
macro_rules! declare_arena {
    ([$($name:ident : $ty:ty),* $(,)?]) => {
        #[derive(Default)]
        pub struct Arena<'tcx> {
            $( pub $name : typed_arena::Arena<$ty>, )*
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

        $(
            impl<'tcx> ArenaAllocatable<'tcx> for $ty {
                #[inline]
                fn allocate_on(self, arena: &'tcx Arena<'tcx>) -> &'tcx Self {
                    arena.$name.alloc(self)
                }
            }
        )*

        $(
            impl<'tcx> ArenaAllocatableMut<'tcx> for $ty {
                #[inline]
                fn allocate_on_mut(self, arena: &'tcx Arena<'tcx>) -> &'tcx mut Self {
                    arena.$name.alloc(self)
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
        }

        unsafe impl<'tcx> Sync for Arena<'tcx> {}
    };
}

#[cfg(test)]
mod tests {
    #[derive(Debug, PartialEq)]
    pub struct Foo(i32);

    #[derive(Debug, PartialEq)]
    pub struct Bar(&'static str);

    // Declare an arena with two types:
    declare_arena!([
        foos: Foo,
        bars: Bar,
    ]);

    #[test]
    fn alloc_single_values() {
        let arena = Arena::default();

        let f = arena.alloc(Foo(1));
        let b = arena.alloc(Bar("hello"));

        assert_eq!(f, &Foo(1));
        assert_eq!(b, &Bar("hello"));
    }

    #[test]
    fn separate_pools_do_not_interfere() {
        let arena = Arena::default();

        let f1 = arena.alloc(Foo(1));
        let b1 = arena.alloc(Bar("x"));
        let f2 = arena.alloc(Foo(2));

        assert_eq!(f1, &Foo(1));
        assert_eq!(f2, &Foo(2));
        assert_eq!(b1, &Bar("x"));
    }

    #[test]
    fn alloc_mut_allows_mutation() {
        let arena = Arena::default();

        let foo = arena.alloc_mut(Foo(1));
        foo.0 = 5;

        assert_eq!(foo, &Foo(5));
    }

    struct Holder<'tcx> {
        foo: &'tcx mut Foo,
    }

    impl Holder<'_> {
        fn bump(&mut self) {
            self.foo.0 += 1;
        }

        fn set_value(&mut self, value: i32) {
            self.foo.0 = value;
        }
    }

    fn increment(foo: &mut Foo) {
        foo.0 += 1;
    }

    fn recursive_decrement(holder: &mut Holder<'_>, remaining: i32) {
        if remaining <= 0 {
            return;
        }
        holder.foo.0 -= 1;
        recursive_decrement(holder, remaining - 1);
    }

    #[test]
    fn alloc_mut_shared_through_holder() {
        let arena = Arena::default();
        let mut holder = Holder {
            foo: arena.alloc_mut(Foo(10)),
        };

        holder.bump();
        assert_eq!(holder.foo.0, 11);

        holder.set_value(25);
        assert_eq!(holder.foo.0, 25);
    }

    #[test]
    fn alloc_mut_passed_to_functions() {
        let arena = Arena::default();
        let mut holder = Holder {
            foo: arena.alloc_mut(Foo(3)),
        };

        increment(holder.foo);
        assert_eq!(holder.foo.0, 4);

        recursive_decrement(&mut holder, 2);
        assert_eq!(holder.foo.0, 2);
    }
}
