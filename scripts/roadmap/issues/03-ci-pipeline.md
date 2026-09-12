---
title: CI: fmt, clippy, test and wasm32-wasip1 build on every PR
milestone: M0 - Scaffold & CI
labels: task,area:ci,priority:p0
---

## Context
As soon as the crate exists, contributions need automated verification so
regressions are caught before merge.

Note a correction to an earlier assumption here. It is true that since
zellij-tile 0.45.0 the host functions are no-ops off-wasm
(`#[cfg(not(target_arch = "wasm32"))] unsafe fn host_run_plugin_command() {}`),
so plugin code *links* natively. But that only makes native tests **possible**,
not **cheap**: `zellij-tile` still drags in the whole of `zellij-utils`, which
under `cfg(not(target_family = "wasm"))` requires `isahc` -> `curl` ->
`curl-sys` + `openssl-sys`, plus `tokio`, `rusqlite`, `notify` and `log4rs`.
A native build of the plugin crate therefore needs apt-installed
`libssl-dev`/`libcurl4-openssl-dev` and takes ~60-70s.

The workspace is split precisely to dodge this (see #2): `zclip-core` is
dependency-free and tests in under a second, while `zclip` is only ever built
for `wasm32-wasip1`, where none of those dependencies exist.

## Acceptance criteria
- [x] A GitHub Actions workflow runs on every push to `main` and every pull request
- [x] `cargo fmt --all -- --check` fails the build on unformatted code
- [x] `cargo clippy -p zclip-core --all-targets -- -D warnings` fails the build on lint warnings
- [x] `cargo test -p zclip-core --all-targets` runs natively with **no** system libraries installed
- [x] `cargo clippy --workspace --target wasm32-wasip1 --all-targets -- -D warnings` lints the plugin crate
- [x] `cargo build --workspace --target wasm32-wasip1 --release` runs in a separate job and fails on compile errors
- [x] The job asserts `target/wasm32-wasip1/release/zclip.wasm` exists and uploads it as a build artifact
- [x] Rust toolchain and cargo registry/build output are cached between runs
- [x] Workflow status badge added to README

## Technical notes
- Use `dtolnay/rust-toolchain` (with `targets: wasm32-wasip1` in the wasm job) and
  `Swatinem/rust-cache` with a distinct `shared-key` per job so caches do not collide
- Split into jobs: `lint` (fmt + clippy), `test` (native `zclip-core` tests),
  `build-wasm` (wasm clippy + release build + artifact) so failures are
  attributable at a glance
- **No `--workspace` or `--all` without `--target wasm32-wasip1`**, and no bare
  `cargo test`/`build`/`clippy` at the workspace root — either would link `zclip`
  natively and reintroduce the openssl/curl requirement this layout exists to avoid.
  Do not "fix" such a failure by adding `apt install`
- **Do not set a global `RUSTFLAGS: -D warnings`.** `RUSTFLAGS` applies to
  dependencies too, so any warning in the `zellij-utils` tree would fail the build.
  Pass `-D warnings` to each clippy invocation instead, which only denies warnings
  in our own crates

## Out of scope
- Release/publish automation (packaging the plugin for distribution)
- End-to-end tests that launch a real Zellij session in CI

## Depends on
Bootstrap the Rust WASM plugin crate
