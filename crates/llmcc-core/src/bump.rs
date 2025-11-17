use std::sync::Arc;
use parking_lot::{RwLock, RwLockWriteGuard, RwLockReadGuard};
use bumpalo_herd::Herd;

use crate::ir::{HirFile, HirIdent};
use crate::symbol::Symbol;
use crate::scope::Scope;

// #[macro_export]
macro_rules! declare_arena {
    ([$($field:ident : $ty:ty),* $(,)?]) => {
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
            pub fn reset(&mut self) {
                $( self.$field.get_mut().clear(); )*
                self.herd.reset();
            }

            /// Thread-safe allocation; no mutable reference needed.
            #[inline]
            pub fn alloc<T: ArenaInsert<'a>>(&'a self, value: T) -> &'a T {
                value.insert_into(self)
            }

            // ====== Auto-generated getters ======
            $(
                paste::paste! {
                    #[inline]
                    pub fn [<$field>](&self) -> Vec<&'a $ty> {
                        self.$field.read().clone()
                    }
                }
            )*
        }

        /// Shared wrapper around `ArenaInner`, safely cloneable across threads.
        #[derive(Clone, Debug)]
        pub struct Arena<'a> {
            inner: Arc<ArenaInner<'a>>,
        }

        impl<'a> Arena<'a> {
            /// Create a new shared Arena.
            pub fn new() -> Self {
                Self { inner: Arc::new(ArenaInner::new()) }
            }

            /// Get the internal Arc.
            #[inline]
            pub fn inner_arc(&self) -> &Arc<ArenaInner<'a>> {
                &self.inner
            }

            /// Try to reset the Arena if uniquely owned.
            pub fn try_reset(self) -> Result<(), Self> {
                match Arc::try_unwrap(self.inner) {
                    Ok(mut inner) => {
                        inner.reset();
                        Ok(())
                    }
                    Err(inner_arc) => Err(Self { inner: inner_arc }),
                }
            }
        }

        // ---- Deref to ArenaInner ----
        impl<'a> Deref for Arena<'a> {
            type Target = ArenaInner<'a>;
            #[inline]
            fn deref(&self) -> &Self::Target {
                &self.inner
            }
        }

        impl<'a> DerefMut for Arena<'a> {
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



// #[cfg(test)]
// mod tests {
//     use super::*;
//     use rayon::prelude::*;

//     fn is_send_sync<T: Send + Sync>() {}

//     #[test]
//     fn test_herd_is_sync() {
//         is_send_sync::<Herd>();
//         is_send_sync::<Arena>();
//     }

//     #[test]
//     fn herd_parallel_alloc_works() {
//         let mut herd = Herd::new();

//         // Parallel computation: each worker gets its own Member<'h>
//         // via herd.get() (map_init), then allocates values in bump.
//         let ints: Vec<&usize> = (0usize..1_000)
//             .into_par_iter()
//             .map_init(
//                 || herd.get(),
//                 |bump, i| {
//                     let r_mut = bump.alloc(i);
//                     &*r_mut
//                 },
//             )
//             .collect();

//         // All 1000 values exist and are accessible here on the main thread.
//         assert_eq!(ints.len(), 1_000);
//         let sum: usize = ints.iter().map(|r| **r).sum();
//         assert_eq!(sum, (0..1_000).sum());

//         // We can still allocate from the herd after the parallel section.
//         let s = herd.get().alloc_str("hello");
//         assert_eq!(s, "hello");
        
//         herd.reset();
        
//         // Test we can resue the herd after reset
//         let ints: Vec<&mut usize> = (0usize..1_000)
//             .into_par_iter()
//             .map_init(
//                 || herd.get(),             // called once per worker thread
//                 |bump, i| bump.alloc(i), // allocates &'h mut usize
//             )
//             .collect();

//         // All 1000 values exist and are accessible here on the main thread.
//         assert_eq!(ints.len(), 1_000);
//         let sum: usize = ints.iter().map(|r| **r).sum();
//         assert_eq!(sum, (0..1_000).sum());

//     }

//    // Dummy types for testing
//     #[derive(Debug, PartialEq)]
//     struct HirRoot(u32);
//     #[derive(Debug, PartialEq)]
//     struct HirIdent<'a>(&'a str);
//     #[derive(Debug, PartialEq)]
//     struct Scope<'a>(&'a str);

//     // Invoke the macro
//     declare_arena!([
//         hir_root: HirRoot,
//         hir_ident: HirIdent<'a>,
//         scope: Scope<'a>,
//     ]);

//     #[test]
//     fn arena_new_initializes_empty() {
//         let arena = Arena::new();
//         assert!(arena.hir_root().is_empty());
//         assert!(arena.hir_ident().is_empty());
//         assert!(arena.scope().is_empty());
//     }

//     #[test]
//     fn alloc_inserts_and_returns_reference() {
//         let arena = Arena::new();

//         let r1 = arena.alloc(HirRoot(1));
//         let r2 = arena.alloc(HirRoot(2));
//         let id = arena.alloc(HirIdent("abc"));
//         let sc = arena.alloc(Scope("main"));

//         // Returned refs should be accessible and correct
//         assert_eq!(r1.0, 1);
//         assert_eq!(r2.0, 2);
//         assert_eq!(id.0, "abc");
//         assert_eq!(sc.0, "main");

//         // They should be pushed into the right Vec
//         assert_eq!(arena.hir_root().len(), 2);
//         assert_eq!(arena.hir_ident().len(), 1);
//         assert_eq!(arena.scope().len(), 1);
//     }

//     #[test]
//     fn iterators_yield_expected_values() {
//         let arena = Arena::new();
//         arena.alloc(HirRoot(10));
//         arena.alloc(HirRoot(20));
//         arena.alloc(HirIdent("x"));
//         arena.alloc(Scope("s1"));
//         arena.alloc(Scope("s2"));

//         let roots: Vec<_> = arena.hir_root();
//         assert_eq!(roots.len(), 2);
//         assert_eq!(roots[0].0, 10);
//         assert_eq!(roots[1].0, 20);

//         let scopes: Vec<_> = arena.scope();
//         assert_eq!(scopes.iter().map(|s| s.0).collect::<Vec<_>>(), vec!["s1", "s2"]);

//         let idents: Vec<_> = arena.hir_ident();
//         assert_eq!(idents[0].0, "x");
//     }

// }
#[cfg(test)]
mod tests {
    use std::sync::{Barrier};
    use std::thread;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Complex struct with lifetime-bound references
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Person<'a> {
        pub id: u32,
        pub name: &'a str,
        pub age: u8,
    }

    // Another complex struct with multiple references
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Document<'a> {
        pub title: &'a str,
        pub author: &'a str,
        pub pages: u16,
    }

    // Struct with nested reference and data
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Task<'a> {
        pub id: u64,
        pub description: &'a str,
        pub priority: u8,
    }

    // Declare test arena with complex structs containing references
    declare_arena!([
        people: Person<'a>,
        documents: Document<'a>,
        tasks: Task<'a>,
        integers: i32
    ]);

    // ============ BASIC FUNCTIONALITY TESTS ============

    #[test]
    fn test_arena_creation() {
        let arena = Arena::new();
        assert_eq!(arena.people().len(), 0);
        assert_eq!(arena.documents().len(), 0);
        assert_eq!(arena.tasks().len(), 0);
        assert_eq!(arena.integers().len(), 0);
    }

    #[test]
    fn test_single_allocation() {
        let arena = Arena::new();
        let person = arena.alloc(Person {
            id: 1,
            name: "Alice",
            age: 30,
        });
        assert_eq!(person.name, "Alice");
        assert_eq!(person.age, 30);
        assert_eq!(arena.people().len(), 1);
    }

    #[test]
    fn test_multiple_allocations_same_type() {
        let arena = Arena::new();
        let p1 = arena.alloc(Person {
            id: 1,
            name: "Alice",
            age: 30,
        });
        let p2 = arena.alloc(Person {
            id: 2,
            name: "Bob",
            age: 25,
        });
        let p3 = arena.alloc(Person {
            id: 3,
            name: "Charlie",
            age: 35,
        });

        assert_eq!(arena.people().len(), 3);
        assert_eq!(p1.name, "Alice");
        assert_eq!(p2.name, "Bob");
        assert_eq!(p3.name, "Charlie");
    }

    #[test]
    fn test_allocations_different_types() {
        let arena = Arena::new();
        let person = arena.alloc(Person {
            id: 1,
            name: "Alice",
            age: 30,
        });
        let doc = arena.alloc(Document {
            title: "Rust Guide",
            author: "Alice",
            pages: 150,
        });
        let task = arena.alloc(Task {
            id: 101,
            description: "Learn Rust",
            priority: 5,
        });
        let num = arena.alloc(42i32);

        assert_eq!(arena.people().len(), 1);
        assert_eq!(arena.documents().len(), 1);
        assert_eq!(arena.tasks().len(), 1);
        assert_eq!(arena.integers().len(), 1);

        assert_eq!(person.name, "Alice");
        assert_eq!(doc.title, "Rust Guide");
        assert_eq!(task.description, "Learn Rust");
        assert_eq!(*num, 42);
    }

    #[test]
    fn test_allocated_references_are_stable() {
        let arena = Arena::new();
        let p1 = arena.alloc(Person {
            id: 1,
            name: "Alice",
            age: 30,
        });
        let p1_ptr = p1 as *const _;

        arena.alloc(Person {
            id: 2,
            name: "Bob",
            age: 25,
        });
        arena.alloc(Person {
            id: 3,
            name: "Charlie",
            age: 35,
        });

        let p1_again = arena.people()[0];
        let p1_again_ptr = p1_again as *const _;

        assert_eq!(p1_ptr, p1_again_ptr);
        assert_eq!(p1_again.name, "Alice");
    }

    #[test]
    fn test_deref_functionality() {
        let arena = Arena::new();
        arena.alloc(Person {
            id: 1,
            name: "Alice",
            age: 30,
        });
        arena.alloc(Person {
            id: 2,
            name: "Bob",
            age: 25,
        });

        let people = arena.people();
        assert_eq!(people.len(), 2);
        assert_eq!(people[0].name, "Alice");
        assert_eq!(people[1].name, "Bob");
    }

    #[test]
    fn test_clone_arena() {
        let arena1 = Arena::new();
        arena1.alloc(Person {
            id: 1,
            name: "Alice",
            age: 30,
        });

        let arena2 = arena1.clone();
        assert_eq!(arena2.people().len(), 1);

        // Both clones share the same underlying data
        arena1.alloc(Person {
            id: 2,
            name: "Bob",
            age: 25,
        });
        assert_eq!(arena2.people().len(), 2);
    }

    #[test]
    fn test_try_reset_succeeds_when_unique() {
        let arena = Arena::new();
        let handle = arena.clone();

        handle.alloc(Person {
            id: 1,
            name: "Alice",
            age: 30,
        });
        assert_eq!(handle.people().len(), 1);

        // consume the original Arc
        let result = arena.try_reset();
        assert!(result.is_ok());

        // verify arena is reset and all data cleared
        assert_eq!(handle.people().len(), 0);
        assert_eq!(handle.documents().len(), 0);
        assert_eq!(handle.tasks().len(), 0);
    }

    // #[test]
    // fn test_try_reset_fails_with_multiple_arcs() {
    //     let arena1 = Arena::new();
    //     arena1.alloc(Person {
    //         id: 1,
    //         name: "Alice",
    //         age: 30,
    //     });

    //     let arena2 = arena1.clone();
    //     let result = arena1.try_reset();

    //     assert!(result.is_err());
    //     let arena1_returned = result.unwrap_err();
    //     assert_eq!(arena1_returned.people().len(), 1);
    // }

    // #[test]
    // fn test_inner_arc_access() {
    //     let arena = Arena::new();
    //     let arc1 = arena.inner_arc();
    //     let arc2 = arena.inner_arc();

    //     // Both should point to same Arc
    //     assert!(Arc::ptr_eq(arc1, arc2));
    // }

    // // ============ MULTI-THREADED TESTS ============

    // #[test]
    // fn test_concurrent_allocations_same_type() {
    //     let arena = Arc::new(Arena::new());
    //     let mut handles = vec![];

    //     for thread_id in 0..4 {
    //         let arena_clone = Arc::clone(&arena);
    //         let handle = thread::spawn(move || {
    //             for i in 0..100 {
    //                 let id = thread_id * 100 + i as u32;
    //                 arena_clone.alloc(Person {
    //                     id,
    //                     name: "Worker",
    //                     age: 25,
    //                 });
    //             }
    //         });
    //         handles.push(handle);
    //     }

    //     for handle in handles {
    //         handle.join().unwrap();
    //     }

    //     assert_eq!(arena.people().len(), 400);
    // }

    // #[test]
    // fn test_concurrent_allocations_multiple_types() {
    //     let arena = Arc::new(Arena::new());
    //     let mut handles = vec![];

    //     for thread_id in 0..4 {
    //         let arena_clone = Arc::clone(&arena);
    //         let handle = thread::spawn(move || {
    //             for i in 0..50 {
    //                 arena_clone.alloc(Person {
    //                     id: i as u32,
    //                     name: "Person",
    //                     age: (20 + i as u8) % 100,
    //                 });
    //                 arena_clone.alloc(Document {
    //                     title: "Doc",
    //                     author: "Author",
    //                     pages: i as u16,
    //                 });
    //                 arena_clone.alloc(Task {
    //                     id: (i * thread_id) as u64,
    //                     description: "Task",
    //                     priority: (i % 10) as u8,
    //                 });
    //             }
    //         });
    //         handles.push(handle);
    //     }

    //     for handle in handles {
    //         handle.join().unwrap();
    //     }

    //     assert_eq!(arena.people().len(), 200);
    //     assert_eq!(arena.documents().len(), 200);
    //     assert_eq!(arena.tasks().len(), 200);
    // }

    // #[test]
    // fn test_concurrent_reads_during_allocations() {
    //     let arena = Arc::new(Arena::new());
    //     let barrier = Arc::new(Barrier::new(5));
    //     let mut handles = vec![];

    //     // 4 allocating threads + 1 reading thread
    //     for thread_id in 0..4 {
    //         let arena_clone = Arc::clone(&arena);
    //         let barrier_clone = Arc::clone(&barrier);
    //         let handle = thread::spawn(move || {
    //             barrier_clone.wait();
    //             for i in 0..1000 {
    //                 arena_clone.alloc(Person {
    //                     id: (thread_id * 1000 + i) as u32,
    //                     name: "Worker",
    //                     age: (20 + (i % 50) as u8) as u8,
    //                 });
    //             }
    //         });
    //         handles.push(handle);
    //     }

    //     // Reader thread
    //     let arena_clone = Arc::clone(&arena);
    //     let barrier_clone = Arc::clone(&barrier);
    //     let reader_handle = thread::spawn(move || {
    //         barrier_clone.wait();
    //         let mut max_len = 0;
    //         for _ in 0..100 {
    //             let current_len = arena_clone.people().len();
    //             max_len = max_len.max(current_len);
    //             thread::yield_now();
    //         }
    //         max_len
    //     });

    //     for handle in handles {
    //         handle.join().unwrap();
    //     }

    //     let final_len = arena.people().len();
    //     assert_eq!(final_len, 4000);

    //     let max_observed = reader_handle.join().unwrap();
    //     assert!(max_observed <= final_len);
    // }

    // #[test]
    // fn test_high_contention_stress() {
    //     let arena = Arc::new(Arena::new());
    //     let counter = Arc::new(AtomicUsize::new(0));
    //     let mut handles = vec![];

    //     for thread_id in 0..8 {
    //         let arena_clone = Arc::clone(&arena);
    //         let counter_clone = Arc::clone(&counter);
    //         let handle = thread::spawn(move || {
    //             for i in 0..500 {
    //                 arena_clone.alloc(Document {
    //                     title: "Doc",
    //                     author: "Author",
    //                     pages: ((thread_id * 500 + i) as u16) % 1000,
    //                 });
    //                 counter_clone.fetch_add(1, Ordering::Relaxed);
    //             }
    //         });
    //         handles.push(handle);
    //     }

    //     for handle in handles {
    //         handle.join().unwrap();
    //     }

    //     assert_eq!(arena.documents().len(), 4000);
    //     assert_eq!(counter.load(Ordering::Relaxed), 4000);
    // }

    // #[test]
    // fn test_concurrent_clones_and_allocations() {
    //     let arena = Arc::new(Arena::new());
    //     let mut handles = vec![];

    //     for thread_id in 0..6 {
    //         let arena_clone = Arc::clone(&arena);
    //         let handle = thread::spawn(move || {
    //             if thread_id % 2 == 0 {
    //                 // Allocating threads
    //                 for i in 0..200 {
    //                     arena_clone.alloc(Person {
    //                         id: i as u32,
    //                         name: "Person",
    //                         age: (20 + i as u8) % 100,
    //                     });
    //                 }
    //             } else {
    //                 // Thread that clones and allocates
    //                 let _cloned = arena_clone.clone();
    //                 for i in 0..200 {
    //                     arena_clone.alloc(Task {
    //                         id: (i * thread_id) as u64,
    //                         description: "Task",
    //                         priority: (i % 10) as u8,
    //                     });
    //                 }
    //             }
    //         });
    //         handles.push(handle);
    //     }

    //     for handle in handles {
    //         handle.join().unwrap();
    //     }

    //     assert_eq!(arena.people().len(), 600);
    //     assert_eq!(arena.tasks().len(), 600);
    // }

    // #[test]
    // fn test_memory_coherence_across_threads() {
    //     let arena = Arc::new(Arena::new());
    //     let barrier = Arc::new(Barrier::new(3));

    //     let arena1 = Arc::clone(&arena);
    //     let barrier1 = Arc::clone(&barrier);
    //     let handle1 = thread::spawn(move || {
    //         barrier1.wait();
    //         for i in 0..500 {
    //             arena1.alloc(Person {
    //                 id: i as u32,
    //                 name: "Worker",
    //                 age: (20 + i as u8) % 100,
    //             });
    //         }
    //     });

    //     let arena2 = Arc::clone(&arena);
    //     let barrier2 = Arc::clone(&barrier);
    //     let handle2 = thread::spawn(move || {
    //         barrier2.wait();
    //         for i in 0..500 {
    //             arena2.alloc(Document {
    //                 title: "Doc",
    //                 author: "Author",
    //                 pages: i as u16,
    //             });
    //         }
    //     });

    //     barrier.wait();
    //     thread::sleep(std::time::Duration::from_millis(100));

    //     let people_mid = arena.people();
    //     let docs_mid = arena.documents();

    //     handle1.join().unwrap();
    //     handle2.join().unwrap();

    //     let people_final = arena.people();
    //     let docs_final = arena.documents();

    //     assert_eq!(people_final.len(), 500);
    //     assert_eq!(docs_final.len(), 500);
    //     assert!(people_mid.len() <= people_final.len());
    //     assert!(docs_mid.len() <= docs_final.len());
    // }

    // #[test]
    // fn test_panic_safety_in_threads() {
    //     let arena = Arc::new(Arena::new());

    //     let arena_clone = Arc::clone(&arena);
    //     let handle = thread::spawn(move || {
    //         for i in 0..100 {
    //             arena_clone.alloc(Task {
    //                 id: i as u64,
    //                 description: "Task",
    //                 priority: (i % 10) as u8,
    //             });
    //             if i == 50 {
    //                 panic!("Controlled panic at iteration 50");
    //             }
    //         }
    //     });

    //     let result = handle.join();
    //     assert!(result.is_err());

    //     // Arena should still be functional
    //     arena.alloc(Task {
    //         id: 999,
    //         description: "Recovery task",
    //         priority: 10,
    //     });
    //     assert!(arena.tasks().len() >= 1);
    // }

    // #[test]
    // fn test_reference_validity_across_threads() {
    //     let arena = Arc::new(Arena::new());

    //     let person = arena.alloc(Person {
    //         id: 12345,
    //         name: "Original",
    //         age: 30,
    //     });
    //     let person_ptr = person as *const _;

    //     let arena_clone = Arc::clone(&arena);
    //     let handle = thread::spawn(move || {
    //         arena_clone.alloc(Person {
    //             id: 111,
    //             name: "Other1",
    //             age: 25,
    //         });
    //         arena_clone.alloc(Person {
    //             id: 222,
    //             name: "Other2",
    //             age: 35,
    //         });

    //         let all_people = arena_clone.people();
    //         // Original reference should still be valid and findable
    //         all_people.iter().any(|r| *r as *const _ == person_ptr)
    //     });

    //     let found = handle.join().unwrap();
    //     assert!(found);
    // }

    // #[test]
    // fn test_arc_strong_count_behavior() {
    //     let arena1 = Arena::new();
    //     assert_eq!(Arc::strong_count(arena1.inner_arc()), 1);

    //     let arena2 = arena1.clone();
    //     assert_eq!(Arc::strong_count(arena1.inner_arc()), 2);

    //     let arena3 = arena2.clone();
    //     assert_eq!(Arc::strong_count(arena1.inner_arc()), 3);

    //     drop(arena2);
    //     assert_eq!(Arc::strong_count(arena1.inner_arc()), 2);

    //     drop(arena3);
    //     assert_eq!(Arc::strong_count(arena1.inner_arc()), 1);
    // }

    // #[test]
    // fn test_large_allocation_concurrent() {
    //     let arena = Arc::new(Arena::new());
    //     let mut handles = vec![];

    //     for thread_id in 0..4 {
    //         let arena_clone = Arc::clone(&arena);
    //         let handle = thread::spawn(move || {
    //             for i in 0..100 {
    //                 arena_clone.alloc(Document {
    //                     title: "Title",
    //                     author: "Author",
    //                     pages: ((thread_id * 100 + i) as u16) % 1000,
    //                 });
    //             }
    //         });
    //         handles.push(handle);
    //     }

    //     for handle in handles {
    //         handle.join().unwrap();
    //     }

    //     assert_eq!(arena.documents().len(), 400);
    // }

    // // ============ EDGE CASE TESTS ============

    // #[test]
    // fn test_zero_allocations_after_creation() {
    //     let arena = Arena::new();
    //     assert_eq!(arena.people().len(), 0);
    //     assert_eq!(arena.documents().len(), 0);
    //     assert_eq!(arena.tasks().len(), 0);
    //     assert_eq!(arena.integers().len(), 0);
    // }

    // #[test]
    // fn test_getter_returns_clone_not_reference() {
    //     let arena = Arena::new();
    //     arena.alloc(Person {
    //         id: 1,
    //         name: "Alice",
    //         age: 30,
    //     });

    //     let snapshot1 = arena.people();
    //     arena.alloc(Person {
    //         id: 2,
    //         name: "Bob",
    //         age: 25,
    //     });
    //     let snapshot2 = arena.people();

    //     assert_eq!(snapshot1.len(), 1);
    //     assert_eq!(snapshot2.len(), 2);
    // }

    // #[test]
    // fn test_mixed_empty_and_populated_types() {
    //     let arena = Arena::new();
    //     arena.alloc(Person {
    //         id: 1,
    //         name: "Alice",
    //         age: 30,
    //     });

    //     assert_eq!(arena.people().len(), 1);
    //     assert_eq!(arena.documents().len(), 0);
    //     assert_eq!(arena.tasks().len(), 0);
    //     assert_eq!(arena.integers().len(), 0);
    // }

    // #[test]
    // fn test_deref_mut_panics_with_multiple_arcs() {
    //     let mut arena1 = Arena::new();
    //     arena1.alloc(Person {
    //         id: 1,
    //         name: "Alice",
    //         age: 30,
    //     });

    //     let _arena2 = arena1.clone();

    //     let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    //         let _ = &mut *arena1; // This should panic
    //     }));

    //     assert!(result.is_err());
    // }

    // #[test]
    // fn test_sequential_resets() {
    //     let arena = Arena::new();
    //     arena.alloc(Person {
    //         id: 1,
    //         name: "Alice",
    //         age: 30,
    //     });
    //     assert_eq!(arena.people().len(), 1);

    //     let arena = arena.try_reset().unwrap();
    //     assert_eq!(arena.people().len(), 0);

    //     arena.alloc(Person {
    //         id: 2,
    //         name: "Bob",
    //         age: 25,
    //     });
    //     assert_eq!(arena.people().len(), 1);
    // }

    // #[test]
    // fn test_struct_with_multiple_references() {
    //     let arena = Arena::new();
    //     let doc1 = arena.alloc(Document {
    //         title: "Rust Book",
    //         author: "Carol Nichols",
    //         pages: 500,
    //     });
    //     let doc2 = arena.alloc(Document {
    //         title: "Programming in Go",
    //         author: "John Doe",
    //         pages: 350,
    //     });

    //     assert_eq!(doc1.title, "Rust Book");
    //     assert_eq!(doc1.author, "Carol Nichols");
    //     assert_eq!(doc2.title, "Programming in Go");
    //     assert_eq!(arena.documents().len(), 2);

    //     // Verify both documents maintain their references
    //     let docs = arena.documents();
    //     assert!(docs.iter().any(|d| d.title == "Rust Book"));
    //     assert!(docs.iter().any(|d| d.title == "Programming in Go"));
    // }

    // #[test]
    // fn test_concurrent_mixed_struct_allocations() {
    //     let arena = Arc::new(Arena::new());
    //     let mut handles = vec![];

    //     for thread_id in 0..3 {
    //         let arena_clone = Arc::clone(&arena);
    //         let handle = thread::spawn(move || {
    //             for i in 0..100 {
    //                 match thread_id {
    //                     0 => {
    //                         arena_clone.alloc(Person {
    //                             id: i as u32,
    //                             name: "Worker",
    //                             age: 25,
    //                         });
    //                     }
    //                     1 => {
    //                         arena_clone.alloc(Document {
    //                             title: "Doc",
    //                             author: "Author",
    //                             pages: i as u16,
    //                         });
    //                     }
    //                     _ => {
    //                         arena_clone.alloc(Task {
    //                             id: i as u64,
    //                             description: "Task",
    //                             priority: (i % 10) as u8,
    //                         });
    //                     }
    //                 }
    //             }
    //         });
    //         handles.push(handle);
    //     }

    //     for handle in handles {
    //         handle.join().unwrap();
    //     }

    //     assert_eq!(arena.people().len(), 100);
    //     assert_eq!(arena.documents().len(), 100);
    //     assert_eq!(arena.tasks().len(), 100);
    // }

    // #[test]
    // fn test_arc_strong_count_behavior() {
    //     let arena1 = Arena::new();
    //     assert_eq!(Arc::strong_count(arena1.inner_arc()), 1);

    //     let arena2 = arena1.clone();
    //     assert_eq!(Arc::strong_count(arena1.inner_arc()), 2);

    //     let arena3 = arena2.clone();
    //     assert_eq!(Arc::strong_count(arena1.inner_arc()), 3);

    //     drop(arena2);
    //     assert_eq!(Arc::strong_count(arena1.inner_arc()), 2);

    //     drop(arena3);
    //     assert_eq!(Arc::strong_count(arena1.inner_arc()), 1);
    // }
}
