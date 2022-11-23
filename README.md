# Rodeo

[![Rust Docs](https://img.shields.io/docsrs/rodeo)](https://docs.rs/rodeo/)
[![Rust Build Status](https://img.shields.io/github/workflow/status/polazarus/rodeo/rust)](https://github.com/polazarus/rodeo/actions/workflows/rust.yml)
[![Rust Nightly Build Status](https://img.shields.io/github/workflow/status/polazarus/rodeo/rust-nightly?label=nightly+build)](https://github.com/polazarus/rodeo/actions/workflows/rust-nightly.yml)
![](https://img.shields.io/crates/l/rodeo)

**A dropping untyped arena** based on [bumpalo](https://crates.io/crates/bumpalo):

* _arena_: an allocator object that allows en masse deallocation
* _untyped_: the same allocator object may be used to allocate **any type**, unlike [`typed_arena`](https://crates.io/crates/typed_arena)
* _dropping_: any drop on the allocated data **will be called**, unlike [`bumpalo`](https://crates.io/crates/bumpalo)

## Example

```rust
use rodeo::Rodeo;

struct S;
impl Drop for S {
    fn drop(&mut self) {
        println!("dropping S");
    }
}

{
    let rodeo = Rodeo::new();
    let n = rodeo.alloc(42);
    let r = rodeo.alloc(S);
}
```

prints `dropping S`

## Features and `#[no_std]` Support

* `bumpalo` (default)

    If not selected, you will have to plug your own allocator that implements the trait [`ArenaAlloc`](https://docs.rs/rodeo/latest/rodeo/trait.ArenaAlloc.html).

* `std` (default)

    For now, `rodeo` is mostly a `no_std` crate. But `std` makes debugging a whole lot simpler!

You have to opt-out of `bumpalo` and `std` with `default-features = false`.

## Safety

As a memory management library, this code uses `unsafe` extensively. However, the code is tested and dynamically verified with Miri.

## Verification Strategy

### Tests

Some test scenarios are written with [proptest](https://altsysrq.github.io/proptest-book/).

Run the tests simply with:

```shell
$ cargo test
```

### Miri

[Miri](https://github.com/rust-lang/miri) is an interpreter for MIR (an intermediate representation of Rust) that checks Rust code and in particular _unsafe_ code against the experimental Stacked Borrows memory model.

As of `miri 0.1.0 (c1a859b 2022-11-10)`, Rodeo's tests show no error or warning when run with Miri.

```shell
$ rustup +nightly component add miri # if needed
$ cargo +nightly miri test
$ LEAK=1 cargo +nightly miri test # should leak two buffers
```

## To-Do

- [ ] add generic DST allocation, hide behind feature pending stabilization of [Rust RFC 2580](https://rust-lang.github.io/rfcs/2580-ptr-meta.html)

- [ ] investigate `rodeo`'s use for self-referential structures

## License

Rodeo is distributed under the terms of both the MIT license and the Apache License (Version 2.0).
See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).