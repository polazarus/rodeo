//! Fast dropping arena based on _bumpalo_.

#![no_std]
#![warn(unsafe_op_in_unsafe_fn)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]

use core::alloc::Layout;
use core::cell::Cell;
use core::ptr::{addr_of_mut, NonNull};

extern crate alloc;

#[cfg(feature = "bumpalo")]
pub mod bumpalo;

pub mod fallback;

#[cfg(test)]
mod tests;

/// Arena allocator trait.
///
/// Arena allocator do not have to provide a deallocation method.
/// Everything should be deallocated when the arena is dropped.
pub trait ArenaAlloc {
    /// Error type used when the allocation fails.
    type Error;

    /// Try to allocate memory for the given layout.
    ///
    /// # Errors
    ///
    /// If for whatever reasons the allocation fails, returns the given an error variant will be returned.
    fn try_alloc_layout(&self, layout: Layout) -> Result<NonNull<u8>, Self::Error>;
}

/// Header of a droppable allocation
struct Header {
    /// Previous header
    previous: Option<NonNull<Header>>,

    /// Actual finalizer function
    finalizer: unsafe fn(NonNull<Header>),

    /// Memory layout for debugging purposes only
    #[cfg(debug_assertions)]
    layout: Layout,
}

impl Header {
    unsafe fn ptr<T>(this: NonNull<Self>) -> *mut T {
        #[cfg(debug_assertions)]
        unsafe {
            let this = this.as_ref();

            assert_eq!(
                this.layout,
                Layout::new::<T>(),
                "inconsistent memory layout"
            );
        }

        let ptr = this.as_ptr().cast::<(Self, T)>();
        let ptr: *mut T = unsafe { addr_of_mut!((*ptr).1) };

        #[cfg(debug_assertions)]
        unsafe {
            let this = this.as_ref();

            assert!(
                (ptr as usize) & (this.layout.align() - 1) == 0,
                "not aligned pointer"
            );
        }
        ptr
    }

    /// The generic "drop function".
    unsafe fn drop_finalizer<T>(this: NonNull<Self>) {
        unsafe {
            Self::ptr::<T>(this).drop_in_place();
        }
    }

    fn finalize(header: NonNull<Self>) {
        unsafe {
            let dropper = header.as_ref().finalizer;
            dropper(header);
        }
    }
}

#[cfg(feature = "bumpalo")]
type Alloc = ::bumpalo::Bump;

#[cfg(not(feature = "bumpalo"))]
type Alloc = fallback::LeakingAlloc;

/// A bump-allocator based arena that cleanly drops allocated data.
///
/// # Example
///
/// ```rust
/// use rodeo::Rodeo;
///
/// // Create a new arena.
/// let rodeo = Rodeo::new();
///
/// // Allocate an integer value into the arena.
/// let forty_two = rodeo.alloc(42);
/// assert_eq!(forty_two, &42);
///
/// // Mutable references are returned from the allocation.
/// let n = rodeo.alloc(1);
/// *n = 2;
/// ```
#[derive(Default)]
pub struct Rodeo<A: ArenaAlloc> {
    allocator: A,
    last: Cell<Option<NonNull<Header>>>,
}

impl Rodeo<Alloc> {
    /// Create a new dropping allocator with a default allocator (a [`bumpalo::Bump`] if the `bumpalo` feature is enabled).
    #[must_use]
    pub fn new() -> Self {
        Self {
            allocator: Alloc::default(),
            last: Cell::default(),
        }
    }
}

impl<A> Rodeo<A>
where
    A: ArenaAlloc,
{
    /// Creates a new dropping allocator based on the given arena allocator.
    #[must_use]
    pub fn with_allocator(allocator: A) -> Self {
        Self {
            allocator,
            last: Cell::default(),
        }
    }

    /// Allocate an object in this `Rodeo` and return an exclusive reference to it.
    ///
    /// # Panics
    ///
    /// Panics if reserving space for `T` (and possibly an header) fails.
    pub fn alloc<T>(&self, value: T) -> &mut T {
        let Ok(ref_mut) = self.try_alloc(value) else { oom() };
        ref_mut
    }

    /// Try to allocate an object in this `Rodeo` and return an exclusive reference to it.
    ///
    /// # Errors
    ///
    /// Errors if reserving space for `T` fails.
    pub fn try_alloc<T>(&self, value: T) -> Result<&mut T, A::Error> {
        if core::mem::needs_drop::<T>() {
            self.try_alloc_with_drop(value)
        } else {
            self.try_alloc_without_drop(value)
            // self.allocator.try_alloc(value)
        }
    }

    fn try_alloc_without_drop<T>(&self, value: T) -> Result<&mut T, A::Error> {
        let layout = Layout::new::<T>();
        let mut ptr = self.allocator.try_alloc_layout(layout)?.cast::<T>();
        unsafe {
            ptr.as_ptr().write(value);
            Ok(ptr.as_mut())
        }
    }

    fn try_alloc_with_drop<T>(&self, value: T) -> Result<&mut T, A::Error> {
        let mut header = Header {
            previous: None,
            finalizer: Header::drop_finalizer::<T>,
            #[cfg(debug_assertions)]
            layout: Layout::new::<T>(),
        };

        // allocate enough for the header and the actual value
        let layout = Layout::new::<(Header, T)>();
        let raw = self.allocator.try_alloc_layout(layout)?;
        // NB: for Miri, for now, as of 2022-11-11,
        // it's better to stay with pointers

        // set the header's previous field after allocating successfully
        header.previous = self.last.take();
        let ptr = raw.cast::<(Header, T)>().as_ptr();
        unsafe {
            ptr.write((header, value));
        }

        let header_ptr = unsafe { NonNull::new_unchecked(addr_of_mut!((*ptr).0)) };
        self.last.set(Some(header_ptr));

        Ok(unsafe { &mut (*ptr).1 })
    }

    /// Return a shared reference to the underlying allocator.
    ///
    /// Any object directly allocated with the allocator **will not be dropped**.
    pub const fn allocator(&self) -> &A {
        &self.allocator
    }

    /// "Leak" all allocated data.
    ///
    /// That is, no drop will be done on any previous allocation when the whole `Rodeo` is dropped.
    ///
    /// N.B.: there is no direct memory leak, only indirect memory and resource leak.
    pub fn leak_all(&self) {
        self.last.set(None);
    }
}

#[inline(never)]
#[cold]
fn oom() -> ! {
    panic!("out of memory")
}

impl<A: ArenaAlloc> Drop for Rodeo<A> {
    fn drop(&mut self) {
        let mut current = self.last.get();
        while let Some(header) = current {
            Header::finalize(header);
            current = unsafe { header.as_ref().previous };
        }
    }
}

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
extern "C" {}

pub const HEADER_LAYOUT: Layout = Layout::new::<Header>();
