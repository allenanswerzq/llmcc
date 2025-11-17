use std::sync::Arc;
use bumpalo::{collections::Vec as BumpVec, Bump};
use parking_lot::{RwLock, RwLockWriteGuard, RwLockReadGuard};
use bumpalo_herd::Herd;

#[cfg(test)]
mod tests {
    use super::*;
    use rayon::prelude::*;

    fn herd_is_send_sync<T: Send + Sync>() {}

    #[test]
    fn test_herd_is_sync() {
        herd_is_send_sync::<Herd>();
    }

    #[test]
    fn herd_parallel_alloc_works() {
        let mut herd = Herd::new();

        // Parallel computation: each worker gets its own Member<'h>
        // via herd.get() (map_init), then allocates values in bump.
        let ints: Vec<&mut usize> = (0usize..1_000)
            .into_par_iter()
            .map_init(
                || herd.get(),             // called once per worker thread
                |bump, i| bump.alloc(i), // allocates &'h mut usize
            )
            .collect();

        // All 1000 values exist and are accessible here on the main thread.
        assert_eq!(ints.len(), 1_000);
        let sum: usize = ints.iter().map(|r| **r).sum();
        assert_eq!(sum, (0..1_000).sum());

        // We can still allocate from the herd after the parallel section.
        let s = herd.get().alloc_str("hello");
        assert_eq!(s, "hello");
        
        herd.reset();
        
        // Test we can resue the herd after reset
        let ints: Vec<&mut usize> = (0usize..1_000)
            .into_par_iter()
            .map_init(
                || herd.get(),             // called once per worker thread
                |bump, i| bump.alloc(i), // allocates &'h mut usize
            )
            .collect();

        // All 1000 values exist and are accessible here on the main thread.
        assert_eq!(ints.len(), 1_000);
        let sum: usize = ints.iter().map(|r| **r).sum();
        assert_eq!(sum, (0..1_000).sum());

    }
}
