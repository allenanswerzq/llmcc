//! Thread-safe bumpalo wrapper with pre-allocation support.
//!
//! Fork of bumpalo-herd with configurable initial chunk size.

#![allow(clippy::vec_box)] // Box<Bump> needed for stable addresses when moving between pool and Member

use std::alloc::Layout;
use std::cell::RefCell;
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use bumpalo::Bump;

const DEFAULT_CHUNK_SIZE: usize = 1 << 24; // 16MB
static NEXT_HERD_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug, Default)]
struct HerdInner {
    pooled: Vec<Box<Bump>>,
    thread_local: Vec<Box<Bump>>,
}

/// A group of [`Bump`] allocators with configurable pre-allocation.
#[derive(Debug)]
pub struct Herd {
    inner: Mutex<HerdInner>,
    chunk_size: AtomicUsize,
    id: usize,
}

impl Default for Herd {
    fn default() -> Self {
        Self::new()
    }
}

impl Herd {
    /// Creates a new [`Herd`] with default 16MB chunk size.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HerdInner::default()),
            chunk_size: AtomicUsize::new(DEFAULT_CHUNK_SIZE),
            id: NEXT_HERD_ID.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// Creates a new [`Herd`] with specified initial chunk size per thread.
    pub fn with_chunk_size(chunk_size: usize) -> Self {
        Self {
            inner: Mutex::new(HerdInner::default()),
            chunk_size: AtomicUsize::new(chunk_size),
            id: NEXT_HERD_ID.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// Deallocates all memory from all allocators.
    pub fn reset(&mut self) {
        let inner = self.inner.get_mut().unwrap();
        for bump in inner.pooled.iter_mut().chain(inner.thread_local.iter_mut()) {
            bump.reset();
        }
    }

    /// Allocate from the current thread's cached bump allocator.
    #[inline]
    pub fn alloc<T>(&self, val: T) -> &T {
        let bump = self.thread_local_bump();

        // SAFETY: `thread_local_bump` returns a pointer to a `Bump` owned by
        // this `Herd`. Each thread gets its own cached bump for a herd id, so
        // allocations through this pointer are not shared across threads. The
        // returned allocation is valid until `self` is reset or dropped.
        let allocated = unsafe { bump.as_ref().alloc(val) as *mut T };
        unsafe { &*allocated }
    }

    /// Allocate a string from the current thread's cached bump allocator.
    #[inline]
    pub fn alloc_str(&self, src: &str) -> &str {
        let bump = self.thread_local_bump();

        // SAFETY: Same ownership and thread-locality guarantee as `alloc`.
        let allocated = unsafe { bump.as_ref().alloc_str(src) as *mut str };
        unsafe { &*allocated }
    }

    /// Borrows a member allocator from this herd.
    pub fn get(&self) -> Member<'_> {
        let mut lock = self.inner.lock().unwrap();
        let bump = lock.pooled.pop().unwrap_or_else(|| {
            let size = self.chunk_size.load(Ordering::Relaxed);
            Box::new(Bump::with_capacity(size))
        });
        Member {
            arena: ManuallyDrop::new(bump),
            owner: self,
        }
    }

    fn thread_local_bump(&self) -> NonNull<Bump> {
        thread_local! {
            static BUMPS: RefCell<HashMap<usize, NonNull<Bump>>> = RefCell::new(HashMap::new());
        }

        BUMPS.with(|bumps| {
            if let Some(bump) = bumps.borrow().get(&self.id).copied() {
                return bump;
            }

            let mut bump = Box::new(Bump::with_capacity(self.chunk_size.load(Ordering::Relaxed)));
            let ptr = NonNull::from(bump.as_mut());

            self.inner.lock().unwrap().thread_local.push(bump);
            bumps.borrow_mut().insert(self.id, ptr);
            ptr
        })
    }
}

/// A proxy for a [`Bump`].
#[derive(Debug)]
pub struct Member<'h> {
    arena: ManuallyDrop<Box<Bump>>,
    owner: &'h Herd,
}

macro_rules! alloc_fn {
    ($(pub fn $name: ident<($($g: tt)*)>(&self, $($pname: ident: $pty: ty),*) -> $res: ty;)*) => {
        $(
            pub fn $name<$($g)*>(&self, $($pname: $pty),*) -> $res {
                self.extend(self.arena.$name($($pname),*))
            }
        )*
    }
}

#[allow(missing_docs)]
impl<'h> Member<'h> {
    alloc_fn! {
        pub fn alloc<(T)>(&self, val: T) -> &'h mut T;
        pub fn alloc_with<(T, F: FnOnce() -> T)>(&self, f: F) -> &'h mut T;
        pub fn alloc_str<()>(&self, src: &str) -> &'h mut str;
        pub fn alloc_slice_clone<(T: Clone)>(&self, src: &[T]) -> &'h mut [T];
        pub fn alloc_slice_copy<(T: Copy)>(&self, src: &[T]) -> &'h mut [T];
        pub fn alloc_slice_fill_clone<(T: Clone)>(&self, len: usize, value: &T) -> &'h mut [T];
        pub fn alloc_slice_fill_copy<(T: Copy)>(&self, len: usize, value: T) -> &'h mut [T];
        pub fn alloc_slice_fill_default<(T: Default)>(&self, len: usize) -> &'h mut [T];
        pub fn alloc_slice_fill_with<(T, F: FnMut(usize) -> T)>(&self, len: usize, f: F) -> &'h mut [T];
    }

    pub fn alloc_slice_fill_iter<T, I>(&self, iter: I) -> &'h mut [T]
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: ExactSizeIterator,
    {
        self.extend(self.arena.alloc_slice_fill_iter(iter))
    }

    pub fn alloc_layout(&self, layout: Layout) -> NonNull<u8> {
        self.arena.as_ref().alloc_layout(layout)
    }

    fn extend<'s, T: ?Sized>(&'s self, v: &'s mut T) -> &'h mut T {
        let result = v as *mut T;
        unsafe { &mut *result }
    }

    /// Access the [`Bump`] inside.
    pub fn as_bump(&self) -> &Bump {
        &self.arena
    }
}

impl Drop for Member<'_> {
    fn drop(&mut self) {
        let mut lock = self.owner.inner.lock().unwrap();
        let member = unsafe { ManuallyDrop::take(&mut self.arena) };
        lock.pooled.push(member);
    }
}
