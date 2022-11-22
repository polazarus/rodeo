//! Main tests of [`Rodeo`]

use alloc::vec::Vec;
use core::cell::RefCell;

use crate::fallback::FailingAlloc;

use super::*;

use proptest::prelude::*;

#[derive(Clone)]
struct DropCallback<F: FnMut()>(F);
impl<F: FnMut()> Drop for DropCallback<F> {
    fn drop(&mut self) {
        (self.0)()
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

#[test]
fn test_slice_copy() {
    let rodeo = Rodeo::new();
    let s1 = rodeo.alloc_slice_copy(b"test");
    assert_eq!(s1, b"test");
    let s2 = rodeo.alloc_slice_copy(b"hello");
    assert_eq!(s2, b"hello");
}

#[test]
fn test_str() {
    let rodeo = Rodeo::new();
    let s1 = rodeo.alloc_str("test");
    assert_eq!(s1, "test");
    let s2 = rodeo.alloc_str("hello");
    assert_eq!(s2, "hello");
}

#[test]
fn test_slice_clone_no_drop() {
    #[derive(Clone)]
    struct S(usize);

    let array = [S(1), S(2)];
    {
        let rodeo = Rodeo::new();
        rodeo.alloc_slice_clone(&array);
    }
}

#[test]
fn test_slice_clone_drop_leak() {
    let witness = Cell::new(0);
    let dc = DropCallback(|| witness.set(witness.get() + 1));
    let array = [dc.clone(), dc.clone()];
    {
        let rodeo = Rodeo::new();
        rodeo.alloc_slice_clone(&array);
        rodeo.leak_all();
    }
    assert_eq!(witness.get(), 0);
}

#[test]
fn test_slice_clone_drop() {
    let witness = Cell::new(0);
    let dc = DropCallback(|| witness.set(witness.get() + 1));
    let array = [dc.clone(), dc.clone()];
    {
        let rodeo = Rodeo::new();
        rodeo.alloc_slice_clone(&array);
    }
    assert_eq!(witness.get(), 2);
}

fn check_number_drop(n: u32) {
    let witness = Cell::new(0);

    {
        let rodeo = Rodeo::new();
        for _ in 0..n {
            let _ = rodeo.alloc(DropCallback(|| {
                witness.set(witness.get() + 1);
            }));
        }
        assert_eq!(witness.get(), 0);
    }

    assert_eq!(witness.get(), n);
}

fn check_order_drop(n: u8) {
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
fn test_number_drop_10() {
    check_number_drop(10);
}

#[test]
fn test_order_drop_10() {
    check_order_drop(10);
}

#[test]
fn test_drop_should_not_leak() {
    let rodeo = Rodeo::new();
    let _ = rodeo.alloc(Box::new(
        0xDEAD_BEEF__DEAD_BEEF__DEAD_BEEF__DEAD_BEEF_u128.to_be(),
    ));
    let _ = rodeo.alloc(vec![b'\xAA'; 50]);
    if option_env!("LEAK").is_some() {
        rodeo.leak_all();
    }
}

proptest! {
    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_number_drop(n in 1..2000u32) {
        check_number_drop(n);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_order_drop(n in 2..100u8) {
        check_order_drop(n);
    }
}
