//! Fast dropping arena based on _bumpalo_.

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unsafe_op_in_unsafe_fn)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]

use core::alloc::Layout;
use core::cell::Cell;
use core::ptr::NonNull;

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
    finalizer: unsafe fn(NonNull<u8>),

    /// Memory layout of associated data for debugging purposes only
    #[cfg(debug_assertions)]
    finalizer_data_layout: Layout,

    /// Memory layout for debugging purposes only
    #[cfg(debug_assertions)]
    data_layout: Layout,
}

impl Header {
    fn finalize(header: NonNull<Self>) {
        unsafe {
            let dropper = header.as_ref().finalizer;
            dropper(header.cast());
        }
    }
}

/// The generic "drop function".
unsafe fn drop_finalizer<T>(non_null: NonNull<u8>) {
    let header_layout = Layout::new::<Header>();
    let unit_layout = Layout::new::<()>();
    let t_layout = Layout::new::<T>();

    #[cfg(debug_assertions)]
    {
        let header = unsafe { non_null.cast::<Header>().as_ref() };
        debug_assert_eq!(unit_layout, header.finalizer_data_layout);
        debug_assert_eq!(t_layout, header.data_layout);
    }

    let (layout, _) = header_layout.extend(unit_layout).unwrap();
    let (_, offset_t) = layout.extend(t_layout).unwrap();

    unsafe {
        let bytes = non_null.as_ptr();
        let ptr: *mut T = bytes.wrapping_add(offset_t).cast();
        ptr.drop_in_place();
    }
}

unsafe fn slice_drop_finalizer<T>(non_null: NonNull<u8>) {
    let header_layout = Layout::new::<Header>();
    let len_layout = Layout::new::<usize>();
    let t_layout = Layout::new::<T>();

    let (layout, offset_len) = header_layout.extend(len_layout).unwrap();
    let (_, offset_t) = layout.extend(t_layout).unwrap();

    unsafe {
        let ptr = non_null.as_ptr();
        let len = *ptr.wrapping_add(offset_len).cast();

        #[cfg(debug_assertions)]
        {
            let header = non_null.cast::<Header>().as_ref();
            let slice_layout =
                Layout::from_size_align_unchecked(t_layout.size() * len, t_layout.align());
            debug_assert_eq!(len_layout, header.finalizer_data_layout);
            debug_assert_eq!(slice_layout, header.data_layout);
        }

        let ptr: *mut T = ptr.wrapping_add(offset_t).cast();
        let slice = core::slice::from_raw_parts_mut(ptr, len);
        core::ptr::drop_in_place(slice);
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
        Self::with_allocator(Alloc::default())
    }
}

impl<A> Rodeo<A>
where
    A: ArenaAlloc,
{
    /// Create a new dropping allocator based on the given arena allocator.
    #[must_use]
    pub const fn with_allocator(allocator: A) -> Self {
        Self {
            allocator,
            last: Cell::new(None),
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

    /// Allocate a slice by copying the input slice in this `Rodeo` and return
    /// an exclusive reference to it.
    ///
    /// # Panics
    ///
    /// Panics if reserving space for the slice fails.
    pub fn alloc_slice_copy<T: Copy>(&self, value: &[T]) -> &mut [T] {
        let Ok(ref_mut) = self.try_alloc_slice_copy(value) else { oom() };
        ref_mut
    }

    /// Allocate a slice by cloning the input slice and return an exclusive
    /// reference to it.
    ///
    /// # Panics
    ///
    /// Panics if reserving space for the slice fails.
    pub fn alloc_slice_clone<T: Clone>(&self, value: &[T]) -> &mut [T] {
        let Ok(ref_mut) = self.try_alloc_slice_clone(value) else { oom() };
        ref_mut
    }

    /// Allocate a string slice by copying an input string slice and return
    /// an exclusive reference to it.
    ///
    /// # Panics
    ///
    /// Panics if reserving space for the slice fails.
    pub fn alloc_str(&self, value: &str) -> &mut str {
        let Ok(ref_mut) = self.try_alloc_str(value) else { oom() };
        ref_mut
    }

    /// Try to allocate an object in this allocator  and return an exclusive
    /// reference to it.
    ///
    /// # Errors
    ///
    /// Errors if reserving space for `T` fails.
    pub fn try_alloc<T>(&self, value: T) -> Result<&mut T, A::Error> {
        let ptr: *mut T = if core::mem::needs_drop::<T>() {
            let raw =
                self.try_alloc_layout_with_finalizer(Layout::new::<T>(), drop_finalizer::<T>, ())?;
            raw.cast()
        } else {
            let layout = Layout::new::<T>();
            self.allocator.try_alloc_layout(layout)?.cast().as_ptr()
        };
        unsafe {
            ptr.write(value);
            Ok(&mut *ptr)
        }
    }

    #[inline]
    fn try_alloc_layout_with_finalizer<D>(
        &self,
        data_layout: Layout,
        finalizer: unsafe fn(NonNull<u8>),
        finalizer_data: D,
    ) -> Result<*mut u8, A::Error> {
        let header_layout = Layout::new::<Header>();
        let finalizer_data_layout = Layout::new::<D>();
        let (hdr_fd_layout, fd_offset) = header_layout.extend(finalizer_data_layout).unwrap();
        let (full_layout, data_offset) = hdr_fd_layout.extend(data_layout).unwrap();

        // allocate enough for the header and the actual value
        let ptr = self.allocator.try_alloc_layout(full_layout)?.as_ptr();

        let header = Header {
            previous: self.last.take(),
            finalizer,
            #[cfg(debug_assertions)]
            finalizer_data_layout,
            #[cfg(debug_assertions)]
            data_layout,
        };

        let header_non_null;
        let value_ptr;
        let finalizer_data_ptr;

        unsafe {
            #[allow(clippy::cast_ptr_alignment)]
            let header_ptr = ptr.cast::<Header>();

            header_ptr.write(header);
            header_non_null = NonNull::new_unchecked(header_ptr);

            finalizer_data_ptr = ptr.wrapping_add(fd_offset).cast::<D>();
            finalizer_data_ptr.write(finalizer_data);

            value_ptr = ptr.wrapping_add(data_offset);
        }

        self.last.set(Some(header_non_null));

        Ok(value_ptr)
    }

    /// Try to allocate a string slice by copying an input string slice and return
    /// an exclusive reference to it.
    ///
    /// # Errors
    ///
    /// Fails if reserving space for the slice fails.
    #[inline]
    pub fn try_alloc_str(&self, string: &str) -> Result<&mut str, A::Error> {
        let bytes = self.try_alloc_slice_copy(string.as_bytes())?;
        Ok(unsafe { core::str::from_utf8_unchecked_mut(bytes) })
    }

    /// Try to allocate a slice by copying the input slice in this `Rodeo` and return
    /// an exclusive reference to it.
    ///
    /// # Errors
    ///
    /// Fails if reserving space for the slice fails.
    pub fn try_alloc_slice_copy<T: Copy>(&self, slice: &[T]) -> Result<&mut [T], A::Error> {
        debug_assert!(!core::mem::needs_drop::<T>());

        let len = slice.len();
        let ptr = self.allocator.try_alloc_layout(Layout::for_value(slice))?;
        let ptr: *mut T = ptr.cast().as_ptr();

        unsafe {
            core::ptr::copy_nonoverlapping(slice.as_ptr(), ptr, len);
            Ok(core::slice::from_raw_parts_mut(ptr, len))
        }
    }

    /// Try to allocate a slice by cloning the input slice and return
    /// an exclusive reference to it.
    ///
    /// # Errors
    ///
    /// Fails if reserving space for the slice fails.
    pub fn try_alloc_slice_clone<T: Clone>(&self, slice: &[T]) -> Result<&mut [T], A::Error> {
        let len = slice.len();

        if core::mem::needs_drop::<T>() {
            let finalizer = slice_drop_finalizer::<T>;
            let ptr =
                self.try_alloc_layout_with_finalizer(Layout::for_value(slice), finalizer, len)?;
            let ptr: *mut T = ptr.cast();

            unsafe {
                let progress = Cell::new(0);
                let guard = DropCallback(|| {
                    let to_cleanup = progress.get();
                    for i in 0..to_cleanup {
                        ptr.wrapping_add(i).drop_in_place();
                    }
                });

                for (i, item) in slice.iter().enumerate() {
                    ptr.wrapping_add(i).write(item.clone());
                    progress.set(progress.get() + 1);
                }

                core::mem::forget(guard);
                Ok(core::slice::from_raw_parts_mut(ptr, len))
            }
        } else {
            let ptr = self.allocator.try_alloc_layout(Layout::for_value(slice))?;
            let ptr: *mut T = ptr.cast().as_ptr();

            unsafe {
                for (i, item) in slice.iter().enumerate() {
                    ptr.wrapping_add(i).write(item.clone());
                }
                Ok(core::slice::from_raw_parts_mut(ptr, len))
            }
        }
    }

    /// Return a shared reference to the underlying allocator.
    ///
    /// Any object directly allocated with the allocator **will not be dropped**.
    pub const fn allocator(&self) -> &A {
        &self.allocator
    }

    /// Convert into the underlying allocator without dropping any allocated
    /// droppable data.
    ///
    /// ⚠️ No drop will be done on any previous allocation of this Rodeo.
    ///
    /// N.B.: there is no direct memory leak, only indirect memory and resource leak.
    pub fn into_allocator(self) -> A {
        let alloc = unsafe { std::ptr::read(&self.allocator) };
        std::mem::forget(self);
        alloc
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

struct DropCallback<F: FnMut()>(F);

impl<F: FnMut()> Drop for DropCallback<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}
