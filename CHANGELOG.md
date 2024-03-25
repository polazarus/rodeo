# Changelog

All notable changes to this project will be documented in this file.

## [0.2.1]

### Bug Fixes

- Fix unsoundness issue [#1](https://github.com/polazarus/rodeo/issues/1)

## [0.2.0]

### Bug Fixes

- Add missing deps and bench declaration
- Fix clippy warning

### Features

- Better tests
- Add miri leak test
- Improve README
- Add `package.rust-version`

### Refactor

- Better Github actions
- **breaking**: Replace `leak_all` by `into_allocator` in `Rodeo`
- Clean up code and tests
- Remove proptest

## [0.1.1] - 2022-11-18

### Features

- Add slice allocs (clone, copy and str)
- Add Github Actions (build, build nightly, clippy)
- Add `Rodeo::leak_all`
- Add some benchmarks
- Constify `Rodeo::with_allocator`

### Refactor

- Simplify `try_alloc_with_drop`
- Add `inline` to `Bump`'s `Arena:Alloc` impl
- Prepare for slice finalizer

## [0.1.0] - 2022-11-15

First MVP release
