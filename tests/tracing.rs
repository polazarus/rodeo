use std::alloc::Layout;
use std::sync::{Arc, Mutex};

use bumpalo::{AllocErr, Bump};
use rodeo::{ArenaAlloc, Rodeo, HEADER_LAYOUT};

struct Alloc(Bump, Arc<Mutex<Vec<Layout>>>);

impl ArenaAlloc for Alloc {
    type Error = AllocErr;
    fn try_alloc_layout(&self, layout: Layout) -> Result<std::ptr::NonNull<u8>, Self::Error> {
        let mut guard = self.1.lock().unwrap();
        let result = self.0.try_alloc_layout(layout)?;
        guard.push(layout);
        Ok(result)
    }
}

#[test]
fn test_tracing() {
    let layouts = Arc::new(Mutex::new(Vec::new()));
    let rodeo = Rodeo::with_allocator(Alloc(Bump::new(), layouts.clone()));

    let _ = rodeo.alloc(1_u32);

    let _ = rodeo.alloc(Box::new(40_u64));

    let _ = rodeo.alloc(());

    let g = layouts.lock().unwrap();
    assert_eq!(
        &[
            Layout::new::<u32>(),
            HEADER_LAYOUT.extend(Layout::new::<Box<u64>>()).unwrap().0,
            Layout::new::<()>(),
        ],
        g.as_slice()
    );
}
