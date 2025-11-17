use bumpalo::Bump;
use std::sync::Arc;

#[derive(Debug)]
pub struct ArenaCtxt {
    main_bump: Arc<Bump>,
    unit_bumps: Vec<Arc<Bump>>,
}

impl ArenaCtxt {
    pub fn new() -> Self {
        Self {
            main_bump: Arc::new(Bump::new()),
            unit_bumps: Vec::new(),
        }
    }

    /// Access the main bump allocator
    pub fn main_bump(&self) -> Arc<Bump> {
        Arc::clone(&self.main_bump)
    }

    /// Get a previously-created unit bump by index
    pub fn get_unit_bump(&self, index: usize) -> Arc<Bump> {
        self.unit_bumps
            .get(index)
            .expect("unit bump index out of bounds")
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_ctxt_new() {
        let ctxt = ArenaCtxt::new();
        let bump = ctxt.main_bump();
        // Verify bump was created successfully
        let val = bump.alloc(1usize);
        assert_eq!(*val, 1);
    }

    #[test]
    fn test_main_bump_allocation() {
        let ctxt = ArenaCtxt::new();
        let bump = ctxt.main_bump();

        // Allocate some values
        let val = bump.alloc(42usize);
        assert_eq!(*val, 42);

        let s = bump.alloc_str("hello");
        assert_eq!(s, "hello");

        let arr = bump.alloc_slice_fill_iter(vec![1, 2, 3].into_iter());
        assert_eq!(arr, &[1, 2, 3]);
    }

    #[test]
    fn test_arc_bump_shared_allocation() {
        let bump = Arc::new(Bump::new());

        let bump1 = Arc::clone(&bump);
        let bump2 = Arc::clone(&bump);

        let val1 = bump1.alloc(111usize);
        let val2 = bump2.alloc(222usize);

        assert_eq!(*val1, 111);
        assert_eq!(*val2, 222);

        // Both values should be in the same arena
        assert_eq!(*bump.alloc(333), 333);
    }

    #[test]
    fn test_arc_multiple_clones() {
        let ctxt = ArenaCtxt::new();

        let bump1 = ctxt.main_bump();
        let bump2 = ctxt.main_bump();

        // Both should point to the same allocation
        assert!(Arc::ptr_eq(&bump1, &bump2));

        let val1 = bump1.alloc(999usize);
        let val2 = bump2.alloc(888usize);

        assert_eq!(*val1, 999);
        assert_eq!(*val2, 888);
    }

    #[test]
    fn test_concurrent_arc_access() {
        // This test verifies that Arc<Bump> can be safely shared
        let ctxt = Arc::new(ArenaCtxt::new());

        // Simulate concurrent read access by cloning the Arc
        let ctxt1 = Arc::clone(&ctxt);
        let ctxt2 = Arc::clone(&ctxt);

        let bump1 = ctxt1.main_bump();
        let bump2 = ctxt2.main_bump();

        // Both should refer to the same bump
        assert!(Arc::ptr_eq(&bump1, &bump2));

        bump1.alloc(123usize);
        bump2.alloc(456usize);
    }
}
