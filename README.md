# Rodeo

A dropping arena based on [bumpalo](https://crates.io/crates/bumpalo).

## Example

```rust
use rodeo::Rodeo;

let rodeo = Rodeo::new();

let _ref_n = rodeo.alloc(42);

struct S;
impl Drop for S {
    fn drop(&mut self) {
        println!("dropping S");
    }
}
let _ref = rodeo.alloc(S);

drop(rodeo);
```

prints `dropping S`

## Features

* `bumpalo` (default)

## Safety

As a memory management library, this code uses `unsafe` extensively.
However, the code is tested and dynamically verified.

## Verification strategy

### Tests

Some test scenarios are written with [proptest](https://altsysrq.github.io/proptest-book/).

Run the tests simply with:

```shell
$ cargo test
```

### Miri

Miri is a dynamic verification tool for Rust.

As of `miri 0.1.0 (c1a859b 2022-11-10)`, Rodeo's tests emits no error or warning when run with Miri.

```shell
$ cargo +nightly miri test
```

## License

Rodeo is distributed under the terms of both the MIT license and the Apache License (Version 2.0).
See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).