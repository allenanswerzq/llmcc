/// Simple adopted from the rustc_arena
///
#[macro_export]
macro_rules! declare_arena {
    ([$($name:ident : $ty:ty),* $(,)?]) => {
        #[derive(Default)]
        pub struct Arena<'tcx> {
            $( pub $name : typed_arena::Arena<$ty>, )*
            _marker: std::marker::PhantomData<&'tcx ()>,
        }

        pub trait ArenaAllocatable<'tcx>: Sized {
            fn allocate_on(self, arena: &'tcx Arena<'tcx>) -> &'tcx Self;
        }

        $(
            impl<'tcx> ArenaAllocatable<'tcx> for $ty {
                #[inline]
                fn allocate_on(self, arena: &'tcx Arena<'tcx>) -> &'tcx Self {
                    arena.$name.alloc(self)
                }
            }
        )*

        impl<'tcx> Arena<'tcx> {
            #[inline]
            pub fn alloc<T: ArenaAllocatable<'tcx>>(&'tcx self, value: T) -> &'tcx T {
                value.allocate_on(self)
            }
        }
    };
}

#[cfg(test)]
mod tests {
    #[derive(Debug, PartialEq)]
    struct Foo(i32);

    #[derive(Debug, PartialEq)]
    struct Bar(&'static str);

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
}
