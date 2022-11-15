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
struct Droppable {
    /// Previous header
    previous: Option<NonNull<Droppable>>,

    /// Actual drop function
    dropper: unsafe fn(NonNull<Droppable>),

    /// Memory layout for debugging purposes only
    #[cfg(debug_assertions)]
    layout: Layout,
}

impl Droppable {
    /// The generic "drop function".
    unsafe fn drop<T>(mut this: NonNull<Self>) {
        unsafe {
            let this = this.as_mut();

            debug_assert_eq!(
                this.layout,
                Layout::new::<T>(),
                "inconsistent memory layout"
            );
        }

        let ptr = this.as_ptr().cast::<(Self, T)>();
        let ptr: *mut T = unsafe { addr_of_mut!((*ptr).1) };

        unsafe {
            let this = this.as_ref();
            debug_assert!(
                (ptr as usize) & (this.layout.align() - 1) == 0,
                "not aligned pointer"
            );
        }

        unsafe {
            ptr.drop_in_place();
        }
    }

    fn call_dropper(droppable: NonNull<Self>) {
        let dropper = unsafe { droppable.as_ref().dropper };
        unsafe {
            dropper(droppable);
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
    last: Cell<Option<NonNull<Droppable>>>,
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
        let mut droppable = Droppable {
            previous: None,
            dropper: Droppable::drop::<T>,
            #[cfg(debug_assertions)]
            layout: Layout::new::<T>(),
        };

        // allocate enough for the header and the actual value
        let layout = Layout::new::<(Droppable, T)>();
        let raw = self.allocator.try_alloc_layout(layout)?;
        // NB: for Miri, for now, as of 2022-11-11,
        // it's better to stay with pointers

        // set droppable's previous field after allocating successfully
        droppable.previous = self.last.take();
        let ptr = raw.cast::<(Droppable, T)>().as_ptr();
        unsafe {
            ptr.write((droppable, value));
        }

        let droppable_ptr = unsafe { NonNull::new_unchecked(addr_of_mut!((*ptr).0)) };
        self.last.set(Some(droppable_ptr));

        Ok(unsafe { &mut (*ptr).1 })
    }

    /// Return a shared reference to the underlying allocator.
    ///
    /// Any object directly allocated with the allocator **will not be dropped**.
    pub const fn allocator(&self) -> &A {
        &self.allocator
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
        while let Some(droppable) = current {
            Droppable::call_dropper(droppable);
            current = unsafe { droppable.as_ref().previous };
        }
    }
}
