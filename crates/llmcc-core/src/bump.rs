use parking_lot::RwLock;
use bumpalo_herd::Herd;

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
// #[macro_export]
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
            pub fn inner_arc(&self) -> &Arc<ArenaInner<'a>> {
                &self.inner
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

// ============================================================================
// Complex Struct Definitions for Testing
// ============================================================================

/// A simple entity allocated in the arena
#[derive(Debug, Clone)]
pub struct Entity {
    pub id: u32,
    pub name: String,
    pub data: Vec<i32>,
}

/// A container holding references to entities - OWNED version for easier testing
#[derive(Debug, Clone)]
pub struct EntitySet {
    pub entity_ids: Vec<u32>,
    pub metadata: String,
}

/// A simple record for testing
#[derive(Debug, Clone)]
pub struct Record {
    pub id: u64,
    pub owner_id: u32,
    pub related_ids: Vec<u32>,
    pub value: f64,
}

/// A struct with internal RwLock for testing mutable interior access
#[derive(Debug)]
pub struct MutableEntity {
    pub id: u32,
    pub name: String,
    pub state: RwLock<Vec<i32>>,  // Interior mutability through RwLock
}

impl Clone for MutableEntity {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            name: self.name.clone(),
            state: RwLock::new(self.state.read().clone()),
        }
    }
}

impl MutableEntity {
    pub fn new(id: u32, name: String) -> Self {
        Self {
            id,
            name,
            state: RwLock::new(Vec::new()),
        }
    }

    /// Append to state without mutable reference
    pub fn append_state(&self, value: i32) {
        self.state.write().push(value);
    }

    /// Read state without mutable reference
    pub fn read_state(&self) -> Vec<i32> {
        self.state.read().clone()
    }

    /// Get state length without mutable reference
    pub fn state_len(&self) -> usize {
        self.state.read().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rayon::prelude::*;

    // Define a test arena supporting Entity, EntitySet, and MutableEntity
    declare_arena!(TestArena { entities: Entity, entitysets: EntitySet, mutable_entities: MutableEntity });

    #[test]
    fn test_complex_entity_allocation() {
        let arena = TestArena::new();

        let entity = Entity {
            id: 1,
            name: "Hero".to_string(),
            data: vec![10, 20, 30],
        };

        let allocated = arena.alloc(entity);
        assert_eq!(allocated.id, 1);
        assert_eq!(allocated.name, "Hero");
        assert_eq!(allocated.data.len(), 3);
    }

    #[test]
    fn test_entity_set_with_allocation() {
        let arena = TestArena::new();

        let entity_set = EntitySet {
            entity_ids: vec![1, 2, 3],
            metadata: "test_set".to_string(),
        };

        let allocated_set = arena.alloc(entity_set);
        assert_eq!(allocated_set.entity_ids.len(), 3);
        assert_eq!(allocated_set.metadata, "test_set");
    }

    #[test]
    fn test_entities_tracked_in_arena() {
        let arena = TestArena::new();

        arena.alloc(Entity {
            id: 1,
            name: "Entity1".to_string(),
            data: vec![],
        });

        arena.alloc(Entity {
            id: 2,
            name: "Entity2".to_string(),
            data: vec![],
        });

        let entities = arena.entities();
        assert_eq!(entities.len(), 2);
    }

    #[test]
    fn test_entity_sets_tracked_in_arena() {
        let arena = TestArena::new();

        arena.alloc(EntitySet {
            entity_ids: vec![1, 2],
            metadata: "set1".to_string(),
        });

        arena.alloc(EntitySet {
            entity_ids: vec![3, 4],
            metadata: "set2".to_string(),
        });

        let sets = arena.entitysets();
        assert_eq!(sets.len(), 2);
    }

    // ========== Multithreading Tests with Rayon ==========

    #[test]
    fn test_rayon_parallel_allocation() {
        let arena = TestArena::new();

        let _results: Vec<_> = (0..100)
            .into_par_iter()
            .map(|i| {
                let entity = Entity {
                    id: i as u32,
                    name: format!("Entity_{}", i),
                    data: vec![i as i32; 10],
                };
                arena.alloc(entity)
            })
            .collect();

        let entities = arena.entities();
        assert_eq!(entities.len(), 100);
    }

    #[test]
    fn test_rayon_parallel_entity_sets() {
        let arena = TestArena::new();

        // Parallel: create entity sets
        let _sets: Vec<_> = (0..50)
            .into_par_iter()
            .map(|i| {
                let set = EntitySet {
                    entity_ids: vec![i as u32, (i + 1) as u32],
                    metadata: format!("set_{}", i),
                };
                arena.alloc(set)
            })
            .collect();

        let entity_sets = arena.entitysets();
        assert_eq!(entity_sets.len(), 50);
    }

    #[test]
    fn test_rayon_high_contention() {
        let arena = TestArena::new();

        // High contention: many threads allocating to the same arena
        (0..20)
            .into_par_iter()
            .for_each(|thread_id| {
                for item in 0..50 {
                    let _entity = arena.alloc(Entity {
                        id: (thread_id * 50 + item) as u32,
                        name: format!("T{}_I{}", thread_id, item),
                        data: vec![thread_id as i32, item as i32],
                    });
                }
            });

        let entities = arena.entities();
        assert_eq!(entities.len(), 1000); // 20 threads * 50 items
    }

    #[test]
    fn test_rayon_nested_parallelism() {
        let arena = TestArena::new();

        // Outer parallel loop
        let _batches: Vec<_> = (0..10)
            .into_par_iter()
            .map(|batch_id| {
                // Inner parallel loop
                let ids: Vec<_> = (0..10)
                    .into_par_iter()
                    .map(|item_id| {
                        let entity = Entity {
                            id: (batch_id * 10 + item_id) as u32,
                            name: format!("B{}_I{}", batch_id, item_id),
                            data: (0..5).map(|_| item_id as i32).collect(),
                        };
                        arena.alloc(entity).id
                    })
                    .collect();

                // Create a set from this batch
                let set = EntitySet {
                    entity_ids: ids,
                    metadata: format!("batch_{}", batch_id),
                };
                arena.alloc(set)
            })
            .collect();

        let all_entities = arena.entities();
        assert_eq!(all_entities.len(), 100);

        let all_sets = arena.entitysets();
        assert_eq!(all_sets.len(), 10);
    }

    #[test]
    fn test_rayon_data_aggregation() {
        let arena = TestArena::new();

        // Allocate entities in parallel
        let _entities: Vec<_> = (0..100)
            .into_par_iter()
            .map(|i| {
                arena.alloc(Entity {
                    id: i as u32,
                    name: format!("Entity_{}", i),
                    data: (0..10).map(|j| (i * 10 + j) as i32).collect(),
                })
            })
            .collect();

        // Read and aggregate in parallel
        let total: u64 = arena
            .entities()
            .par_iter()
            .map(|e| e.data.len() as u64)
            .sum();

        assert_eq!(total, 1000); // 100 entities * 10 data items
    }

    #[test]
    fn test_mixed_rayon_and_sequential() {
        let arena = TestArena::new();

        // Sequential phase: create base entities
        let base_ids: Vec<_> = (0..10)
            .map(|i| {
                arena.alloc(Entity {
                    id: i as u32,
                    name: format!("Base_{}", i),
                    data: vec![i as i32],
                })
                .id
            })
            .collect();

        // Parallel phase: create sets referencing base entities
        let _sets: Vec<_> = base_ids
            .into_par_iter()
            .map(|base_id| {
                let set = EntitySet {
                    entity_ids: vec![base_id],
                    metadata: format!("referencing_{}", base_id),
                };
                arena.alloc(set)
            })
            .collect();

        let all_entities = arena.entities();
        assert_eq!(all_entities.len(), 10);

        let all_sets = arena.entitysets();
        assert_eq!(all_sets.len(), 10);
    }

    // ========== RwLock Interior Mutability Tests ==========

    #[test]
    fn test_mutable_entity_rwlock_mutation() {
        let arena = TestArena::new();

        let entity = MutableEntity::new(1, "TestEntity".to_string());
        let allocated = arena.alloc(entity);

        // Mutate through immutable reference via RwLock
        allocated.append_state(10);
        allocated.append_state(20);
        allocated.append_state(30);

        // Read back without mutable reference
        let state = allocated.read_state();
        assert_eq!(state, vec![10, 20, 30]);
        assert_eq!(allocated.state_len(), 3);
    }

    #[test]
    fn test_multiple_mutable_entities_parallel_mutation() {
        let arena = TestArena::new();

        // Allocate multiple entities
        let entities: Vec<_> = (0..10)
            .map(|i| {
                arena.alloc(MutableEntity::new(
                    i as u32,
                    format!("Entity_{}", i),
                ))
            })
            .collect();

        // Mutate each entity in parallel through immutable references
        entities.par_iter().for_each(|entity| {
            for j in 0..5 {
                entity.append_state((entity.id as i32 * 100) + j);
            }
        });

        // Verify all entities were mutated
        for entity in &entities {
            assert_eq!(entity.state_len(), 5);
            let state = entity.read_state();
            assert_eq!(state.len(), 5);
            // Verify values match expected pattern
            for (j, &val) in state.iter().enumerate() {
                assert_eq!(val, (entity.id as i32 * 100) + j as i32);
            }
        }
    }

    #[test]
    fn test_rwlock_concurrent_readers_and_writers() {
        let arena = TestArena::new();

        let entity = MutableEntity::new(1, "ConcurrentTest".to_string());
        let allocated = arena.alloc(entity);

        // Parallel phase: writers and readers on same entity
        (0..20).into_par_iter().for_each(|i| {
            if i % 2 == 0 {
                // Even threads: write
                allocated.append_state(i as i32);
            } else {
                // Odd threads: read
                let state = allocated.read_state();
                let _ = state.len(); // Just verify we can read
            }
        });

        // Verify final state has all the writes
        let final_state = allocated.read_state();
        assert_eq!(final_state.len(), 10); // 20 / 2 = 10 writes
    }

    #[test]
    fn test_mutable_entities_tracked_in_arena() {
        let arena = TestArena::new();

        let _entity1 = arena.alloc(MutableEntity::new(1, "E1".to_string()));
        let _entity2 = arena.alloc(MutableEntity::new(2, "E2".to_string()));
        let _entity3 = arena.alloc(MutableEntity::new(3, "E3".to_string()));

        let mutable_entities = arena.mutable_entities();
        assert_eq!(mutable_entities.len(), 3);
    }

    #[test]
    fn test_nested_parallel_with_mutable_entities() {
        let arena = TestArena::new();

        // Allocate a batch of mutable entities
        let _batch_ids: Vec<_> = (0..5)
            .map(|i| {
                let entity = MutableEntity::new(i as u32, format!("Batch_{}", i));
                arena.alloc(entity).id
            })
            .collect();

        // Nested parallel: outer batches, inner operations on shared entities
        (0..10)
            .into_par_iter()
            .for_each(|batch_idx| {
                (0..5).into_par_iter().for_each(|entity_idx| {
                    let entities = arena.mutable_entities();
                    if let Some(entity) = entities.get(entity_idx) {
                        // Mutate through immutable reference
                        entity.append_state((batch_idx * 5 + entity_idx) as i32);
                    }
                });
            });

        // Verify final state
        let final_entities = arena.mutable_entities();
        assert_eq!(final_entities.len(), 5);

        for entity in final_entities {
            // Each entity should have multiple mutations from parallel tasks
            let state_len = entity.state_len();
            assert!(state_len > 0);
        }
    }

    #[test]
    fn test_mutable_entity_state_aggregation() {
        let arena = TestArena::new();

        // Create entities with specific state patterns
        let entities: Vec<_> = (0..5)
            .map(|i| {
                let entity = MutableEntity::new(i as u32, format!("Entity_{}", i));
                let allocated = arena.alloc(entity);

                // Fill state sequentially
                for j in 0..10 {
                    allocated.append_state((i * 10 + j) as i32);
                }
                allocated
            })
            .collect();

        // Aggregate all states in parallel
        let total_values: i32 = entities
            .par_iter()
            .map(|entity| entity.read_state().iter().sum::<i32>())
            .sum();

        // Expected: sum of (0..10) + (10..20) + ... + (40..50)
        // = sum of (0..50) = 1225
        assert_eq!(total_values, 1225);
    }

    #[test]
    fn test_mutable_entity_with_other_types() {
        let arena = TestArena::new();

        // Mix all types in the arena
        let _entity = arena.alloc(Entity {
            id: 1,
            name: "Regular".to_string(),
            data: vec![1, 2, 3],
        });

        let _mutable = arena.alloc(MutableEntity::new(1, "Mutable".to_string()));

        let _set = arena.alloc(EntitySet {
            entity_ids: vec![1, 2],
            metadata: "set".to_string(),
        });

        // Verify all are tracked separately
        assert_eq!(arena.entities().len(), 1);
        assert_eq!(arena.mutable_entities().len(), 1);
        assert_eq!(arena.entitysets().len(), 1);
    }

    // ========== Demonstrating Thread-Local Allocation + Shared Tracking ==========

    #[test]
    fn test_thread_local_allocation_shared_tracking() {
        let arena = TestArena::new();

        // High-contention scenario: 100 threads each allocating 100 items
        // This demonstrates:
        // 1. Each thread has its own allocator (from Herd) - NO CONTENTION
        // 2. All threads push to the same Vec - MINIMAL CONTENTION (only when pushing)
        (0..100)
            .into_par_iter()
            .for_each(|thread_id| {
                for item_idx in 0..100 {
                    let _entity = arena.alloc(Entity {
                        id: (thread_id * 100 + item_idx) as u32,
                        name: format!("T{}_I{}", thread_id, item_idx),
                        data: vec![thread_id as i32; 10],
                    });
                }
            });

        // Verify all 10,000 allocations are tracked
        let entities = arena.entities();
        assert_eq!(entities.len(), 10000);

        // Verify we can read from all of them without locks on allocation
        let total_data_items: u64 = entities
            .par_iter()
            .map(|e| e.data.len() as u64)
            .sum();
        assert_eq!(total_data_items, 100000); // 10000 entities * 10 items each
    }

    #[test]
    fn test_allocator_isolation() {
        let arena = TestArena::new();

        // Each thread allocates, they all have separate thread-local Bumps
        // But we can still enumerate all allocations
        let results: Vec<_> = (0..10)
            .into_par_iter()
            .map(|thread_id| {
                // Each thread allocates 50 items in its own Bump
                let mut local_count = 0;
                for i in 0..50 {
                    let _entity = arena.alloc(Entity {
                        id: (thread_id * 50 + i) as u32,
                        name: format!("Thread_{}_Item_{}", thread_id, i),
                        data: vec![(thread_id * 50 + i) as i32],
                    });
                    local_count += 1;
                }
                local_count
            })
            .collect();

        // Verify each thread allocated 50 items
        assert_eq!(results.iter().sum::<usize>(), 500);

        // Verify the shared Vec has all 500 items
        let total_in_arena = arena.entities();
        assert_eq!(total_in_arena.len(), 500);

        // And we can access them all without locks
        for entity in total_in_arena {
            assert!(entity.id < 500);
        }
    }

    #[test]
    fn test_drop_member_does_not_free_allocation() {
        // This test demonstrates that drop(member) only drops the GUARD,
        // not the allocated memory. The allocated memory stays valid because
        // it's managed by the thread-local Bump in the Herd.
        let arena = TestArena::new();

        // Allocate an entity
        let entity = Entity {
            id: 42,
            name: "TestEntity".to_string(),
            data: vec![1, 2, 3, 4, 5],
        };
        let allocated = arena.alloc(entity);

        // At this point, inside alloc():
        // 1. member = arena.herd.get() - gets the guard to thread-local Bump
        // 2. r = member.alloc(self) - allocates, r points to the allocated memory
        // 3. drop(member) - drops the GUARD (not the memory!)
        // 4. arena.entities.write().push(r) - pushes the reference
        // The allocated memory is STILL VALID because:
        // - It's stored in the thread-local Bump
        // - The Bump is part of the Herd, which lives as long as the arena
        // - Dropping the guard doesn't deallocate; it just releases the lock

        // Verify we can still access the allocated data after the guard is dropped
        assert_eq!(allocated.id, 42);
        assert_eq!(allocated.name, "TestEntity");
        assert_eq!(allocated.data, vec![1, 2, 3, 4, 5]);

        // Verify it's in the arena's tracking
        let entities = arena.entities();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].id, 42);
    }

    #[test]
    fn test_herd_keeps_memory_alive() {
        // The Herd manages multiple thread-local Bumps.
        // Each Bump is kept alive as long as the Herd exists.
        // When we drop(member), we're just dropping the GUARD to access it,
        // not the Bump itself.
        let arena = TestArena::new();

        let entity_refs: Vec<_> = (0..1000)
            .map(|i| {
                arena.alloc(Entity {
                    id: i as u32,
                    name: format!("Entity_{}", i),
                    data: (0..100).map(|j| (i * 100 + j) as i32).collect(),
                })
            })
            .collect();

        // All 1000 entities are still accessible
        // Even though drop(member) was called 1000 times
        let tracked = arena.entities();
        assert_eq!(tracked.len(), 1000);

        // All references are still valid
        for (i, entity_ref) in entity_refs.iter().enumerate() {
            assert_eq!(entity_ref.id, i as u32);
            assert_eq!(entity_ref.data.len(), 100);
        }
    }

    #[test]
    fn test_drop_before_vs_after_push_correctness() {
        // This test demonstrates that it doesn't matter for CORRECTNESS
        // whether we drop(member) before or after push.
        // The reference r is just a pointer, and the memory stays valid
        // either way because it's managed by the Herd.
        let arena = TestArena::new();

        // Allocate with drop(member) happening BEFORE push (current implementation)
        let entity1 = arena.alloc(Entity {
            id: 1,
            name: "Entity1".to_string(),
            data: vec![1, 2, 3],
        });

        // Both approaches work fine
        assert_eq!(entity1.id, 1);
        assert_eq!(entity1.name, "Entity1");
        assert_eq!(entity1.data.len(), 3);

        // The tracked list has it
        let tracked = arena.entities();
        assert_eq!(tracked.len(), 1);

        // Both drop orders are valid - the only difference is lock contention:
        // drop(member) BEFORE push: Release guard quickly, then contend for RwLock
        // drop(member) AFTER push: Hold guard while acquiring RwLock
        // The current order (drop before) is slightly better for multi-threaded performance
    }

    // ========== Memory Management Model Tests ==========

    #[test]
    fn test_each_thread_has_own_bump() {
        // Each thread gets its own Bump allocator from the Herd.
        // This means:
        // - Thread 1 allocates from Bump 1's memory pool
        // - Thread 2 allocates from Bump 2's memory pool
        // - No cross-thread memory interference
        let arena = TestArena::new();

        let results: Vec<_> = (0..4)
            .into_par_iter()
            .map(|thread_id| {
                // Each thread allocates 100 items from its own Bump
                let mut local_ids = Vec::new();
                for i in 0..100 {
                    let entity = arena.alloc(Entity {
                        id: (thread_id * 1000 + i) as u32,
                        name: format!("T{}_{}", thread_id, i),
                        data: vec![thread_id as i32],
                    });
                    local_ids.push(entity.id);
                }
                local_ids
            })
            .collect();

        // Verify each thread's allocations are distinct
        assert_eq!(results.len(), 4); // 4 threads
        for (thread_id, ids) in results.iter().enumerate() {
            assert_eq!(ids.len(), 100); // Each thread allocated 100
            // IDs in thread 0 are 0-99, thread 1 is 1000-1099, etc.
            for (i, &id) in ids.iter().enumerate() {
                assert_eq!(id as usize, thread_id * 1000 + i);
            }
        }

        // But they're all in the same shared tracking Vec
        let all_entities = arena.entities();
        assert_eq!(all_entities.len(), 400); // 4 threads * 100 items
    }

    #[test]
    fn test_thread_local_memory_pools_independent() {
        // Demonstrate that each thread's Bump is independent.
        // Thread A's allocations don't affect Thread B's memory pool.
        let arena = TestArena::new();

        // Simulate heavy allocation in thread 1, light in thread 2
        let heavy_entities = (0..2)
            .into_par_iter()
            .map(|thread_id| {
                if thread_id == 0 {
                    // Thread 0: allocate a lot of large entities
                    (0..50)
                        .map(|i| {
                            arena.alloc(Entity {
                                id: i as u32,
                                name: format!("Heavy_{}", i),
                                data: (0..1000).collect(), // Large data
                            })
                        })
                        .collect::<Vec<_>>()
                } else {
                    // Thread 1: allocate a few small entities
                    (0..5)
                        .map(|i| {
                            arena.alloc(Entity {
                                id: i as u32,
                                name: format!("Light_{}", i),
                                data: vec![0], // Small data
                            })
                        })
                        .collect::<Vec<_>>()
                }
            })
            .collect::<Vec<_>>();

        // Verify both completed successfully despite different load
        assert_eq!(heavy_entities.len(), 2);
        assert_eq!(heavy_entities[0].len(), 50);
        assert_eq!(heavy_entities[1].len(), 5);

        // All are tracked in the same place
        let all = arena.entities();
        assert_eq!(all.len(), 55); // 50 + 5

        // Heavy allocations still have their large data
        for entity in &heavy_entities[0] {
            assert_eq!(entity.data.len(), 1000);
        }
    }

    #[test]
    fn test_memory_stays_alive_after_thread_exits() {
        // Even though a thread exits, its allocated memory stays alive
        // because it's managed by the Herd, which is part of the Arena
        // (which lives in the test function).
        let arena = TestArena::new();

        // Use rayon thread pool to allocate
        let entity_refs: Vec<_> = (0..100)
            .into_par_iter()
            .map(|i| {
                arena.alloc(Entity {
                    id: i as u32,
                    name: format!("Entity_{}", i),
                    data: vec![i as i32; 10],
                })
            })
            .collect();

        // rayon thread pool tasks have ended by now, but:
        // - Entity data is still valid (thread-local Bumps still exist in Herd)
        // - We still hold valid references
        for (i, entity) in entity_refs.iter().enumerate() {
            assert_eq!(entity.id, i as u32);
            assert_eq!(entity.data.len(), 10);
            assert_eq!(entity.data[0], i as i32);
        }

        // Tracked entities are still accessible
        let tracked = arena.entities();
        assert_eq!(tracked.len(), 100);
    }

    #[test]
    fn test_herd_manages_multiple_bumps() {
        // The Herd maintains a thread-local Bump for each thread.
        // Even with many threads, each gets its own independent pool.
        let arena = TestArena::new();

        // Use 20 threads, each allocating heavily
        (0..20)
            .into_par_iter()
            .for_each(|thread_id| {
                for item in 0..50 {
                    let _entity = arena.alloc(Entity {
                        id: (thread_id * 50 + item) as u32,
                        name: format!("T{}_I{}", thread_id, item),
                        data: (0..100).collect(),
                    });
                }
            });

        // All 1000 allocations succeeded (20 threads * 50 items)
        let all_entities = arena.entities();
        assert_eq!(all_entities.len(), 1000);

        // Each has its full data
        for entity in all_entities {
            assert_eq!(entity.data.len(), 100);
        }
    }

    #[test]
    fn test_memory_layout_per_thread() {
        // This demonstrates the memory management:
        // - Thread-local Bumps manage their own memory pools
        // - References are just pointers into those pools
        // - The shared Vec collects all pointers
        let arena = TestArena::new();

        // Sequential allocation (single thread)
        let seq_entities: Vec<_> = (0..10)
            .map(|i| {
                arena.alloc(Entity {
                    id: i as u32,
                    name: format!("Seq_{}", i),
                    data: vec![i as i32; 20],
                })
            })
            .collect();

        // Parallel allocation (multiple threads)
        let par_entities: Vec<_> = (0..10)
            .into_par_iter()
            .map(|i| {
                arena.alloc(Entity {
                    id: (100 + i) as u32,
                    name: format!("Par_{}", i),
                    data: vec![(100 + i) as i32; 20],
                })
            })
            .collect();

        // All are valid - mixed allocation still works
        assert_eq!(seq_entities.len(), 10);
        assert_eq!(par_entities.len(), 10);

        // All tracked
        let tracked = arena.entities();
        assert_eq!(tracked.len(), 20);

        // Verify we can access all of them
        for entity in tracked {
            assert_eq!(entity.data.len(), 20);
        }
    }
}
