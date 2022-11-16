//! Re-export of `bumpalo` crate and support for Rodeo.

use core::alloc::Layout;
use core::ptr::NonNull;

#[doc(no_inline)]
pub use ::bumpalo::*;

use super::ArenaAlloc;

impl ArenaAlloc for Bump {
    type Error = AllocErr;

    #[inline(always)]
    fn try_alloc_layout(&self, layout: Layout) -> Result<NonNull<u8>, Self::Error> {
        self.try_alloc_layout(layout)
    }
}

/// Convenient alias for a bumpalo-back Rodeo.
pub type Rodeo = crate::Rodeo<Bump>;

#[test]
fn test_bump() {
    let bump = Bump::new();

    let _ = bump.alloc(1);

    drop(bump);
}
