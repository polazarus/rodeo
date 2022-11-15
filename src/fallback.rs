//! Fallback arena allocators for debugging purposes.

use alloc::alloc::alloc;
use core::alloc::Layout;
use core::ptr::NonNull;

use crate::ArenaAlloc;

/// Leaking arena allocator.
#[derive(Default)]
pub struct LeakingAlloc;

/// Allocation error.
pub struct AllocErr;

impl ArenaAlloc for LeakingAlloc {
    type Error = AllocErr;
    fn try_alloc_layout(&self, layout: Layout) -> Result<NonNull<u8>, Self::Error> {
        NonNull::new(unsafe { alloc(layout) }).ok_or(AllocErr)
    }
}
