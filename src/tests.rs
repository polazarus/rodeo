use alloc::vec::Vec;
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
