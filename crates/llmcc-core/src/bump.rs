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
                    #[inline]
                    pub fn [<$field>](&self) -> Vec<&'a $ty> {
                        self.$field.read().clone()
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
                    drop(member);
                    arena.$field.write().push(r);
                    r
                }
            }
        )*
    };
}

#[cfg(test)]
mod tests {
    use crate::interner::InternPool;
    use crate::ir::{HirBase, HirIdent, HirKind, HirScope};
    use crate::scope::Scope;
    use crate::symbol::Symbol;
    use rayon::prelude::*;

    // Define a test arena supporting HirIdent, HirScope, Symbol, and Scope
    declare_arena!(TestArena {
        hir_idents: HirIdent<'a>,
        hir_scopes: HirScope<'a>,
        symbols: Symbol,
        scopes: Scope<'a>,
    });

    fn create_test_hir_id(idx: u32) -> crate::ir::HirId {
        crate::ir::HirId(idx as usize)
    }

    fn create_test_base(id: u32) -> HirBase {
        HirBase {
            id: create_test_hir_id(id),
            parent: None,
            kind_id: 0,
            start_byte: 0,
            end_byte: 0,
            kind: HirKind::Internal,
            field_id: 0,
            children: Vec::new(),
        }
    }

    #[test]
    fn test_arena_allocate_symbol() {
        let arena = TestArena::new();
        let pool = InternPool::new();

        let name = pool.intern("test_symbol");
        let symbol = Symbol::new(create_test_hir_id(1), name);
        let allocated = arena.alloc(symbol);

        // Verify allocation
        assert_eq!(allocated.name, name);
        assert!(allocated.id.0 > 0, "Symbol ID should be positive");

        // Verify tracked in arena
        let symbols = arena.symbols();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].id, allocated.id);
    }

    #[test]
    fn test_arena_allocate_multiple_symbols() {
        let arena = TestArena::new();
        let pool = InternPool::new();

        // Allocate multiple symbols
        let mut allocated_syms = Vec::new();
        for i in 0..10 {
            let name = pool.intern(format!("symbol_{}", i));
            let sym = Symbol::new(create_test_hir_id(i as u32), name);
            let sym_ref = arena.alloc(sym);
            allocated_syms.push(sym_ref);
        }

        // Verify all tracked
        let symbols = arena.symbols();
        assert_eq!(symbols.len(), 10);

        for (i, &sym) in allocated_syms.iter().enumerate() {
            assert!(sym.id.0 > 0, "Symbol {} should have positive ID", i);
        }
    }

    #[test]
    fn test_arena_allocate_scope() {
        let arena = TestArena::new();

        let scope = Scope::new(create_test_hir_id(100));
        let _scope_ref = arena.alloc(scope);

        // Verify allocation
        let scopes = arena.scopes();
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].owner(), create_test_hir_id(100));
    }

    #[test]
    fn test_arena_allocate_hir_ident() {
        let arena = TestArena::new();

        let base = create_test_base(200);
        let hir_ident = HirIdent::new(base, "test_ident".to_string());
        let _ident_ref = arena.alloc(hir_ident);

        // Verify allocation
        let idents = arena.hir_idents();
        assert_eq!(idents.len(), 1);
        assert_eq!(idents[0].name, "test_ident");
    }

    #[test]
    fn test_arena_allocate_hir_scope() {
        let arena = TestArena::new();

        let base = create_test_base(300);
        let hir_scope = HirScope::new(base, None);
        let _scope_ref = arena.alloc(hir_scope);

        // Verify allocation
        let hir_scopes = arena.hir_scopes();
        assert_eq!(hir_scopes.len(), 1);
        assert_eq!(hir_scopes[0].base.id, create_test_hir_id(300));
    }

    #[test]
    fn test_arena_mixed_allocation_pattern() {
        let arena = TestArena::new();
        let pool = InternPool::new();

        // Allocate symbols
        let sym1_name = pool.intern("symbol_1");
        let symbol1 = Symbol::new(create_test_hir_id(1), sym1_name);
        let sym1_ref = arena.alloc(symbol1);

        let sym2_name = pool.intern("symbol_2");
        let symbol2 = Symbol::new(create_test_hir_id(2), sym2_name);
        let sym2_ref = arena.alloc(symbol2);

        // Allocate scope
        let scope = Scope::new(create_test_hir_id(100));
        let scope_ref = arena.alloc(scope);

        // Add symbols to scope
        scope_ref.insert(sym1_ref);
        scope_ref.insert(sym2_ref);

        // Allocate HIR types
        let base1 = create_test_base(200);
        let hir_ident = HirIdent::new(base1, "test_ident".to_string());
        let _ident_ref = arena.alloc(hir_ident);

        let base2 = create_test_base(300);
        let hir_scope = HirScope::new(base2, None);
        let _hir_scope_ref = arena.alloc(hir_scope);

        // Verify all allocations
        assert_eq!(arena.symbols().len(), 2);
        assert_eq!(arena.scopes().len(), 1);
        assert_eq!(arena.hir_idents().len(), 1);
        assert_eq!(arena.hir_scopes().len(), 1);

        // Verify scope contains symbols
        let found_symbols = scope_ref.lookup_symbols(sym1_name);
        assert_eq!(found_symbols.len(), 1);
        assert_eq!(found_symbols[0].id, sym1_ref.id);
    }

    #[test]
    fn test_arena_lifetime_bound_references() {
        // Demonstrate that arena-allocated references are tied to arena lifetime

        let arena = TestArena::new();
        let pool = InternPool::new();

        // Allocate symbols
        let sym_name = pool.intern("lifetime_test");
        let symbol = Symbol::new(create_test_hir_id(1), sym_name);
        let sym_ref = arena.alloc(symbol);

        // Allocate scope and add symbol
        let scope = Scope::new(create_test_hir_id(100));
        let scope_ref = arena.alloc(scope);
        scope_ref.insert(sym_ref);

        // References from scope are still valid (lifetime 'a tied to arena)
        let found = scope_ref.lookup_symbols(sym_name);
        assert_eq!(found[0].id, sym_ref.id);

        // All symbols from arena are valid
        let all_symbols = arena.symbols();
        assert_eq!(all_symbols.len(), 1);
        assert_eq!(all_symbols[0].id, sym_ref.id);
    }

    #[test]
    fn test_arena_default_construction() {
        // Test both new() and default() work
        let arena1 = TestArena::new();
        let arena2 = TestArena::default();

        let pool = InternPool::new();

        let sym1 = Symbol::new(create_test_hir_id(1), pool.intern("sym1"));
        let _ref1 = arena1.alloc(sym1);

        let sym2 = Symbol::new(create_test_hir_id(1), pool.intern("sym2"));
        let _ref2 = arena2.alloc(sym2);

        assert_eq!(arena1.symbols().len(), 1);
        assert_eq!(arena2.symbols().len(), 1);
    }

    #[test]
    fn test_arena_allocate_and_retrieve_all_types() {
        let arena = TestArena::new();
        let pool = InternPool::new();

        // Allocate 3 symbols
        for i in 0..3 {
            let name = pool.intern(format!("sym_{}", i));
            let symbol = Symbol::new(create_test_hir_id(i as u32), name);
            let _ = arena.alloc(symbol);
        }

        // Allocate 2 scopes
        for i in 0..2 {
            let scope = Scope::new(create_test_hir_id(100 + i as u32));
            let _ = arena.alloc(scope);
        }

        // Allocate 2 HirIdents
        for i in 0..2 {
            let base = create_test_base(200 + i as u32);
            let ident = HirIdent::new(base, format!("ident_{}", i));
            let _ = arena.alloc(ident);
        }

        // Allocate 2 HirScopes
        for i in 0..2 {
            let base = create_test_base(300 + i as u32);
            let hir_scope = HirScope::new(base, None);
            let _ = arena.alloc(hir_scope);
        }

        // Verify all are tracked
        assert_eq!(arena.symbols().len(), 3);
        assert_eq!(arena.scopes().len(), 2);
        assert_eq!(arena.hir_idents().len(), 2);
        assert_eq!(arena.hir_scopes().len(), 2);
    }

    #[test]
    fn test_arena_scope_with_multiple_symbols() {
        let arena = TestArena::new();
        let pool = InternPool::new();

        // Create scope
        let scope = Scope::new(create_test_hir_id(100));
        let scope_ref = arena.alloc(scope);

        // Create and add multiple symbols to scope
        let mut symbol_refs = Vec::new();
        for i in 0..50 {
            let name = pool.intern(format!("var_{}", i));
            let symbol = Symbol::new(create_test_hir_id(1000 + i as u32), name);
            let sym_ref = arena.alloc(symbol);
            scope_ref.insert(sym_ref);
            symbol_refs.push((name, sym_ref));
        }

        // Verify all symbols in scope
        for (name, sym_ref) in symbol_refs {
            let found = scope_ref.lookup_symbols(name);
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].id, sym_ref.id);
        }

        // Verify total count
        assert_eq!(arena.symbols().len(), 50);
    }

    #[test]
    fn test_arena_hir_ident_with_symbol_association() {
        let arena = TestArena::new();
        let pool = InternPool::new();

        // Allocate symbol
        let sym_name = pool.intern("test_symbol");
        let symbol = Symbol::new(create_test_hir_id(1), sym_name);
        let sym_ref = arena.alloc(symbol);

        // Allocate HirIdent
        let base = create_test_base(200);
        let hir_ident = HirIdent::new(base, "test_ident".to_string());
        let ident_ref = arena.alloc(hir_ident);

        // Associate symbol with ident
        ident_ref.set_symbol(sym_ref);

        // Verify association
        let idents = arena.hir_idents();
        assert_eq!(idents.len(), 1);
        assert_eq!(idents[0].symbol().id, sym_ref.id);
    }

    #[test]
    fn test_arena_parallel_symbol_allocation() {
        // Test parallel allocation of symbols across multiple threads
        let arena = TestArena::new();
        let pool = InternPool::new();

        // Allocate 100 symbols in parallel
        (0..100).into_par_iter().for_each(|i| {
            let name = pool.intern(format!("parallel_sym_{}", i));
            let symbol = Symbol::new(create_test_hir_id(i as u32), name);
            let _ = arena.alloc(symbol);
        });

        // Verify all symbols were allocated
        let symbols = arena.symbols();
        assert_eq!(symbols.len(), 100);
    }

    #[test]
    fn test_arena_parallel_scope_allocation() {
        // Test parallel allocation of scopes across multiple threads
        let arena = TestArena::new();

        // Allocate 50 scopes in parallel
        (0..50).into_par_iter().for_each(|i| {
            let scope = Scope::new(create_test_hir_id(i as u32));
            let _ = arena.alloc(scope);
        });

        // Verify all scopes were allocated
        let scopes = arena.scopes();
        assert_eq!(scopes.len(), 50);
    }

    #[test]
    fn test_arena_parallel_mixed_allocation() {
        // Test parallel allocation of mixed types across multiple threads
        let arena = TestArena::new();
        let pool = InternPool::new();

        // Allocate symbols and scopes in parallel
        (0..30).into_par_iter().for_each(|i| {
            if i % 2 == 0 {
                // Allocate symbol
                let name = pool.intern(format!("sym_{}", i));
                let symbol = Symbol::new(create_test_hir_id(i as u32), name);
                let _ = arena.alloc(symbol);
            } else {
                // Allocate scope
                let scope = Scope::new(create_test_hir_id(i as u32));
                let _ = arena.alloc(scope);
            }
        });

        // Verify both types were allocated
        let symbols = arena.symbols();
        let scopes = arena.scopes();

        assert_eq!(symbols.len(), 15, "Should have 15 symbols");
        assert_eq!(scopes.len(), 15, "Should have 15 scopes");
    }

    #[test]
    fn test_arena_parallel_hir_allocation() {
        // Test parallel allocation of HIR types across multiple threads
        let arena = TestArena::new();

        // Allocate HirIdents and HirScopes in parallel
        (0..40).into_par_iter().for_each(|i| {
            let base = create_test_base(i as u32);
            if i % 2 == 0 {
                let hir_ident = HirIdent::new(base, format!("ident_{}", i));
                let _ = arena.alloc(hir_ident);
            } else {
                let hir_scope = HirScope::new(base, None);
                let _ = arena.alloc(hir_scope);
            }
        });

        // Verify both types were allocated
        let idents = arena.hir_idents();
        let hir_scopes = arena.hir_scopes();

        assert_eq!(idents.len(), 20, "Should have 20 HirIdents");
        assert_eq!(hir_scopes.len(), 20, "Should have 20 HirScopes");
    }

    #[test]
    fn test_arena_parallel_all_types_allocation() {
        // Test parallel allocation of all types simultaneously
        let arena = TestArena::new();
        let pool = InternPool::new();

        // Allocate 80 items of various types in parallel
        (0..80).into_par_iter().for_each(|i| {
            let alloc_type = i % 4;
            match alloc_type {
                0 => {
                    let name = pool.intern(format!("sym_{}", i));
                    let symbol = Symbol::new(create_test_hir_id(i as u32), name);
                    let _ = arena.alloc(symbol);
                }
                1 => {
                    let scope = Scope::new(create_test_hir_id(i as u32));
                    let _ = arena.alloc(scope);
                }
                2 => {
                    let base = create_test_base(i as u32);
                    let hir_ident = HirIdent::new(base, format!("ident_{}", i));
                    let _ = arena.alloc(hir_ident);
                }
                3 => {
                    let base = create_test_base(i as u32);
                    let hir_scope = HirScope::new(base, None);
                    let _ = arena.alloc(hir_scope);
                }
                _ => unreachable!(),
            }
        });

        // Verify all types were allocated correctly
        let symbols = arena.symbols();
        let scopes = arena.scopes();
        let idents = arena.hir_idents();
        let hir_scopes = arena.hir_scopes();

        assert_eq!(symbols.len(), 20, "Should have 20 symbols");
        assert_eq!(scopes.len(), 20, "Should have 20 scopes");
        assert_eq!(idents.len(), 20, "Should have 20 HirIdents");
        assert_eq!(hir_scopes.len(), 20, "Should have 20 HirScopes");
    }

    #[test]
    fn test_arena_parallel_high_contention() {
        // Test with very high contention: many threads allocating simultaneously
        let arena = TestArena::new();
        let pool = InternPool::new();

        // Allocate 200 symbols in parallel with high contention
        (0..200).into_par_iter().for_each(|i| {
            let name = pool.intern(format!("high_contention_{}", i));
            let symbol = Symbol::new(create_test_hir_id(i as u32), name);
            let _ = arena.alloc(symbol);
        });

        let symbols = arena.symbols();
        assert_eq!(symbols.len(), 200, "All 200 symbols should be allocated");
    }

    #[test]
    fn test_arena_parallel_lifetime_safety() {
        // Verify that arena references remain valid across parallel operations
        let arena = TestArena::new();
        let pool = InternPool::new();

        // Phase 1: Allocate symbols and scopes in parallel
        (0..50).into_par_iter().for_each(|i| {
            let name = pool.intern(format!("verify_sym_{}", i));
            let symbol = Symbol::new(create_test_hir_id(i as u32), name);
            let _ = arena.alloc(symbol);

            let scope = Scope::new(create_test_hir_id(100 + i as u32));
            let _ = arena.alloc(scope);
        });

        // Phase 2: Verify all references are still valid
        let symbols = arena.symbols();
        let scopes = arena.scopes();

        assert_eq!(symbols.len(), 50);
        assert_eq!(scopes.len(), 50);

        // Verify we can still access symbol properties
        for symbol in symbols.iter() {
            assert!(symbol.id.0 > 0);
            // Can call methods on arena-allocated references
            let _ = symbol.kind();
        }
    }

    #[test]
    fn test_arena_parallel_cloned_arc_allocation() {
        // Test that cloned Arena references can allocate independently
        let arena_original = TestArena::new();
        let pool = InternPool::new();

        // Create multiple clones of the arena (Arc is already inside)
        let arena_clone1 = arena_original.clone();
        let arena_clone2 = arena_original.clone();
        let arena_clone3 = arena_original.clone();

        // Allocate in parallel using different clones
        rayon::scope(|s| {
            s.spawn(|_| {
                (0..25).into_par_iter().for_each(|i| {
                    let name = pool.intern(format!("thread1_sym_{}", i));
                    let symbol = Symbol::new(create_test_hir_id(i as u32), name);
                    let _ = arena_clone1.alloc(symbol);
                });
            });
            s.spawn(|_| {
                (25..50).into_par_iter().for_each(|i| {
                    let name = pool.intern(format!("thread2_sym_{}", i));
                    let symbol = Symbol::new(create_test_hir_id(i as u32), name);
                    let _ = arena_clone2.alloc(symbol);
                });
            });
            s.spawn(|_| {
                (50..75).into_par_iter().for_each(|i| {
                    let name = pool.intern(format!("thread3_sym_{}", i));
                    let symbol = Symbol::new(create_test_hir_id(i as u32), name);
                    let _ = arena_clone3.alloc(symbol);
                });
            });
        });

        // Verify all allocations succeeded
        let symbols = arena_original.symbols();
        assert_eq!(
            symbols.len(),
            75,
            "All symbols from all threads should be present"
        );
    }
}
