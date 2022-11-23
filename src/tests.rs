//! Main tests of [`Rodeo`]

use alloc::vec::Vec;
use core::cell::RefCell;

use crate::fallback::FailingAlloc;

use super::*;

#[derive(Clone)]
struct DropCallback<F: FnMut()>(F);
impl<F: FnMut()> Drop for DropCallback<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

#[test]
fn test_no_mem() {
    let rodeo = Rodeo::with_allocator(FailingAlloc);
    assert!(rodeo.try_alloc(42).is_err());

    let witness = Cell::new(false);
    assert!(rodeo
        .try_alloc(DropCallback(|| {
            witness.set(true);
        }))
        .is_err());
    assert!(witness.get());
}

#[test]
#[should_panic]
fn test_no_mem_panic() {
    let rodeo = Rodeo::with_allocator(FailingAlloc);
    let _ = rodeo.alloc(42);
}

#[test]
#[should_panic]
fn test_no_mem_panic_drop() {
    let rodeo = Rodeo::with_allocator(FailingAlloc);
    assert!(rodeo.try_alloc(42).is_err());

    let witness = Cell::new(false);
    let _guard = DropCallback(|| {
        // double panic if witness is not set
        assert!(witness.get());
    });

    let _ = rodeo.alloc(DropCallback(|| {
        witness.set(true);
    }));
}

#[test]
fn test_into_allocator() {
    struct FakeAlloc;
    impl ArenaAlloc for FakeAlloc {
        type Error = ();

        fn try_alloc_layout(&self, _layout: Layout) -> Result<NonNull<u8>, Self::Error> {
            todo!()
        }
    }
    let rodeo = Rodeo::with_allocator(FakeAlloc);
    let _alloc: FakeAlloc = rodeo.into_allocator();
}

#[test]
fn test_into_allocator_drop_not_called() {
    let witness = Cell::new(false);
    let rodeo = Rodeo::new();
    rodeo.alloc(DropCallback(|| witness.set(true)));
    let _alloc = rodeo.into_allocator();
    assert!(!witness.get(), "drop should not be called");
}

#[test]
fn test_drop() {
    let witness = Cell::new(false);
    {
        let rodeo = Rodeo::new();
        let _dcb = rodeo.alloc(DropCallback(|| {
            witness.set(true);
        }));
        assert!(!witness.get());
    }
    assert!(witness.get());
}

#[test]
fn test_alloc_value_eq_no_drop() {
    let rodeo = Rodeo::new();

    // type consistency check
    let &mut () = rodeo.alloc(());

    for n in [1, 2, 3, 42, 0xDEAD_CAFE_u128, 80] {
        let p = rodeo.alloc(n);
        assert_eq!(p, &n);
    }
}

#[test]
fn test_alloc_slice_copy() {
    let rodeo = Rodeo::new();
    let s1 = rodeo.alloc_slice_copy(b"test");
    assert_eq!(s1, b"test");
    let s2 = rodeo.alloc_slice_copy(b"hello");
    assert_eq!(s2, b"hello");
}

#[test]
fn test_alloc_str() {
    let rodeo = Rodeo::new();
    let s1 = rodeo.alloc_str("test");
    assert_eq!(s1, "test");
    let s2 = rodeo.alloc_str("hello");
    assert_eq!(s2, "hello");
}

#[test]
fn test_alloc_slice_clone_no_drop() {
    #[derive(Clone, Eq, PartialEq, Debug)]
    struct S(usize);

    let array = [S(1), S(2)];
    {
        let rodeo = Rodeo::new();
        let slice = rodeo.alloc_slice_clone(&array);
        assert_eq!(slice, &array[..]);
    }
}

#[test]
fn test_alloc_slice_clone_drop_leak() {
    let witness = Cell::new(0);
    let dc = DropCallback(|| witness.set(witness.get() + 1));
    let array = [dc.clone(), dc.clone()];
    {
        let rodeo = Rodeo::new();
        rodeo.alloc_slice_clone(&array);
        let _alloc = rodeo.into_allocator();
    }
    assert_eq!(witness.get(), 0);
}

#[test]
fn test_alloc_slice_clone_drop() {
    let witness = Cell::new(0);
    let dc = DropCallback(|| witness.set(witness.get() + 1));
    let array = [dc.clone(), dc.clone()];
    {
        let rodeo = Rodeo::new();
        rodeo.alloc_slice_clone(&array);
    }
    assert_eq!(witness.get(), 2);
}

fn check_alloc_drop_order(n: u8) {
    let witness = RefCell::new(Vec::with_capacity(n as usize));

    {
        let rodeo = Rodeo::new();
        for i in 0..n {
            let witness = &witness;
            let _ = rodeo.alloc(DropCallback(move || {
                witness.borrow_mut().push(i);
            }));
        }
        assert!(witness.borrow().is_empty());
    }

    let vec = witness.take();
    assert_eq!(vec.len(), n as usize);
    assert!(vec.windows(2).all(|w| w[0] >= w[1]));
}

#[test]
fn test_alloc_drop_order_10() {
    check_alloc_drop_order(10);
}

#[test]
fn test_alloc_drop_order_100() {
    check_alloc_drop_order(100);
}

#[test]
fn test_alloc_slice_drop_order() {
    let n = 10;
    let witness = RefCell::new(Vec::with_capacity(n));

    let objects: Vec<_> = (0..n)
        .map(|i| {
            let witness = &witness;
            DropCallback(move || {
                witness.borrow_mut().push(i);
            })
        })
        .collect();

    {
        let rodeo = Rodeo::new();
        let _clones = rodeo.alloc_slice_clone(&objects[..]);
    }
    let got: Vec<_> = witness.borrow_mut().drain(..).collect();

    // compute the expected drop order
    // i.e. the order when dropping the original objects
    drop(objects);
    let expected: Vec<_> = witness.take();

    assert_eq!(got, expected);
}

#[test]
fn test_drop_should_not_leak() {
    let rodeo = Rodeo::new();
    let _ = rodeo.alloc(Box::new(
        0xDEAD_BEEF_DEAD_BEEF_DEAD_BEEF_DEAD_BEEF_u128.to_be(),
    ));
    let _ = rodeo.alloc(vec![b'\xAA'; 50]);
    if option_env!("LEAK").is_some() {
        let _alloc = rodeo.into_allocator();
    }
}
