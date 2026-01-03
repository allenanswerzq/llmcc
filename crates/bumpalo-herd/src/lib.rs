//! Thread-safe bumpalo wrapper with pre-allocation support.
//!
//! Fork of bumpalo-herd with configurable initial chunk size to reduce malloc calls.

use std::alloc::Layout;
use std::mem::ManuallyDrop;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use bumpalo::Bump;

/// Default chunk size: 1MB per thread for reduced malloc pressure
const DEFAULT_CHUNK_SIZE: usize = 1 << 20; // 1MB

type HerdInner = Vec<Box<Bump>>;

/// A group of [`Bump`] allocators with configurable pre-allocation.
#[derive(Debug)]
pub struct Herd {
    inner: Mutex<HerdInner>,
    chunk_size: AtomicUsize,
}

impl Default for Herd {
    fn default() -> Self {
        Self::new()
    }
}

impl Herd {
    /// Creates a new [`Herd`] with default 1MB chunk size.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Vec::new()),
            chunk_size: AtomicUsize::new(DEFAULT_CHUNK_SIZE),
        }
    }

    /// Creates a new [`Herd`] with specified initial chunk size per thread.
    /// Larger chunks = fewer malloc calls but more memory usage.
    pub fn with_chunk_size(chunk_size: usize) -> Self {
        Self {
            inner: Mutex::new(Vec::new()),
            chunk_size: AtomicUsize::new(chunk_size),
        }
    }

    /// Deallocates all memory from all allocators.
    pub fn reset(&mut self) {
        for e in self.inner.get_mut().unwrap().iter_mut() {
            e.reset();
        }
    }

    /// Borrows a member allocator from this herd.
    pub fn get(&self) -> Member<'_> {
        let mut lock = self.inner.lock().unwrap();
        let bump = lock.pop().unwrap_or_else(|| {
            // Pre-allocate with configured chunk size
            let size = self.chunk_size.load(Ordering::Relaxed);
            Box::new(Bump::with_capacity(size))
        });
        Member {
            arena: ManuallyDrop::new(bump),
            owner: self,
        }
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
        lock.push(member);
    }
}
