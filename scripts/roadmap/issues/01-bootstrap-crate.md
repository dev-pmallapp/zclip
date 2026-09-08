---
title: Bootstrap the Rust WASM plugin crate
milestone: M0 - Scaffold & CI
labels: task,area:ci,priority:p0
---

## Context
The repository currently contains only a LICENSE file. We need a compilable Rust
crate targeting `wasm32-wasip1` that implements the minimal `ZellijPlugin` trait so
that later features have a concrete place to live and so CI has something to
build against.

## Acceptance criteria
- [ ] `Cargo.toml` declares a `cdylib` crate depending on `zellij-tile = "0.45"`
- [ ] A `Zclip` struct derives `Default` and implements `ZellijPlugin`
- [ ] `load`, `update`, `pipe`, and `render` are implemented as minimal stubs (render prints a placeholder string)
- [ ] `register_plugin!(Zclip)` is called at the crate root
- [ ] `cargo build --target wasm32-wasip1` succeeds and produces `target/wasm32-wasip1/debug/zclip.wasm`
- [ ] `rust-toolchain.toml` (or equivalent) pins the toolchain and the `wasm32-wasip1` target
- [ ] `.gitignore` excludes `target/`

## Technical notes
- Crate API reference: https://docs.rs/zellij-tile/latest/zellij_tile/
- `ZellijPlugin` trait signature and lifecycle: https://docs.rs/zellij-tile/latest/zellij_tile/trait.ZellijPlugin.html
  and https://zellij.dev/documentation/plugin-lifecycle.html
- `load(&mut self, configuration: BTreeMap<String,String>)` receives KDL-config-derived key/value pairs
- `update(&mut self, event: Event) -> bool` and `pipe(&mut self, pipe_message: PipeMessage) -> bool` should
  return `true` only when a re-render is required
- Follow the crate layout used by https://github.com/zellij-org/rust-plugin-example as a structural reference
- `Cargo.toml` should set `crate-type = ["cdylib"]`

## Out of scope
- Permission requests and gating (separate issue)
- Any buffer/business logic

## Depends on
None
