//! Fast dropping arena based on _bumpalo_.

#![warn(unsafe_op_in_unsafe_fn)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]

use core::alloc::Layout;
use core::cell::Cell;
use core::ptr::{addr_of_mut, NonNull};

#[cfg(feature = "bumpalo")]
use bumpalo::{AllocErr, Bump};

pub trait ArenaAlloc {
    type Error;
    fn try_alloc_layout(&self, layout: Layout) -> Result<NonNull<u8>, Self::Error>;
}

#[cfg(feature = "bumpalo")]
impl ArenaAlloc for Bump {
    type Error = AllocErr;
    fn try_alloc_layout(&self, layout: Layout) -> Result<NonNull<u8>, Self::Error> {
        self.try_alloc_layout(layout)
    }
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

#[cfg(feature = "bumpalo")]
impl Rodeo<Bump> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            allocator: Bump::new(),
            last: Cell::default(),
        }
    }
}

impl<A: ArenaAlloc> Rodeo<A> {
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

    /// Return the underlying allocator.
    ///
    /// Returns only a shared reference (so as not to be able to reset).
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

impl<A> Drop for Rodeo<A> {
    fn drop(&mut self) {
        let mut current = self.last.get();
        while let Some(droppable) = current {
            Droppable::call_dropper(droppable);
            current = unsafe { droppable.as_ref().previous };
        }
    }
}

#[cfg(test)]
mod tests {
    use core::cell::RefCell;

    use super::*;

    use proptest::prelude::*;

    struct DropCallback<F: FnMut()>(F);
    impl<F: FnMut()> Drop for DropCallback<F> {
        fn drop(&mut self) {
            (self.0)()
        }
    }

    #[test]
    fn test_drop() {
        let mut witness = false;
        {
            let rodeo = Rodeo::new();
            let _dcb = rodeo.alloc(DropCallback(|| {
                witness = true;
            }));
            assert!(!witness);
        }
        assert!(witness);
    }

    #[test]
    fn test_no_drop() {
        let rodeo = Rodeo::new();
        let a = rodeo.alloc(1_u128);
        assert_eq!(a, &1);
        let b = rodeo.alloc(2_u64);
        assert_eq!(b, &2);
        let c = rodeo.alloc(3_u32);
        assert_eq!(c, &3);
        let d = rodeo.alloc(4_u16);
        assert_eq!(d, &4);
        let e = rodeo.alloc(5_u8);
        assert_eq!(e, &5);
        let () = rodeo.alloc(());
    }

    proptest! {

        #[test]
        fn test_number_drop(n in 1..2000u32) {
            let witness = Cell::new(0);

            {
                let rodeo = Rodeo::new();
                for _ in 0..n {
                    let _ = rodeo.alloc(DropCallback(|| {
                        witness.set(witness.get()+1);
                    }));
                }
                assert_eq!(witness.get(), 0);
            }

            assert_eq!(witness.get(), n);
        }

        #[test]
        fn test_order_drop(n in 2..100u8) {
            let witness = RefCell::new(Vec::with_capacity(n as usize));

            {
                let rodeo = Rodeo::new();
                for i in 0..n {
                    let _ = rodeo.alloc(DropCallback(|| {
                        witness.borrow_mut().push(i);
                    }));
                }
                assert!(witness.borrow().is_empty());
            }

            let vec = witness.take();
            prop_assert_eq!(vec.len(), n as usize);
            prop_assert!(vec.windows(2).all(|w| w[0] >= w[1]));
        }
    }

    #[test]
    fn test_bump() {
        let bump = Bump::new();

        let _ = bump.alloc(1);

        drop(bump);
    }
}
