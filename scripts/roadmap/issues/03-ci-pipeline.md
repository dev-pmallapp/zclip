---
title: CI: fmt, clippy, test and wasm32-wasip1 build on every PR
milestone: M0 - Scaffold & CI
labels: task,area:ci,priority:p0
---

## Context
As soon as the crate exists, contributions need automated verification so
regressions are caught before merge. Since zellij-tile 0.45.0, host functions
compile to no-ops on non-wasm targets, so `cargo test` can run natively without a
WASM toolchain — CI should exploit this for fast native unit tests in addition to
the actual wasm32-wasip1 build check.

## Acceptance criteria
- [ ] A GitHub Actions workflow runs on every push and pull request
- [ ] `cargo fmt --check` fails the build on unformatted code
- [ ] `cargo clippy --all-targets -- -D warnings` fails the build on lint warnings
- [ ] `cargo test` runs on the native host target (no wasm toolchain required)
- [ ] `cargo build --target wasm32-wasip1` runs in a separate job/step and fails the build on compile errors
- [ ] Rust toolchain and `wasm32-wasip1` target installation are cached between runs
- [ ] Workflow status badge added to README

## Technical notes
- zellij-tile 0.45+ no-ops host functions off-wasm, enabling native `cargo test`: https://docs.rs/zellij-tile/latest/zellij_tile/
- Use `actions-rs`/`dtolnay/rust-toolchain` (or equivalent maintained action) with
  `rustup target add wasm32-wasip1`
- Split into jobs: `lint` (fmt+clippy), `test` (native cargo test), `build-wasm` (wasm32-wasip1 build)
  so failures are attributable at a glance

## Out of scope
- Release/publish automation (packaging the plugin for distribution)
- End-to-end tests that launch a real Zellij session in CI

## Depends on
Bootstrap the Rust WASM plugin crate
