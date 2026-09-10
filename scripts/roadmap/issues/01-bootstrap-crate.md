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
- [ ] A Cargo **workspace** with two members: `crates/zclip-core` and `crates/zclip`
- [ ] `crates/zclip-core` has **zero dependencies** and holds all host-independent logic
- [ ] `crates/zclip` is a **binary** crate (`[[bin]]`, `src/main.rs`) depending on `zellij-tile = "0.45"` and `zclip-core`
- [ ] A `Zclip` struct derives `Default` and implements `ZellijPlugin`
- [ ] `load`, `update`, `pipe`, and `render` are implemented as minimal stubs (render prints a placeholder string)
- [ ] `register_plugin!(Zclip)` is called at the crate root
- [ ] `cargo build --workspace --target wasm32-wasip1` succeeds and produces `target/wasm32-wasip1/debug/zclip.wasm`
- [ ] `cargo test -p zclip-core` passes with no system libraries installed
- [ ] `rust-toolchain.toml` (or equivalent) pins the toolchain and the `wasm32-wasip1` target
- [ ] `.gitignore` excludes `target/`

## Why a workspace and not a single crate
`zellij-tile` depends on `zellij-utils`, which is Zellij's *entire* shared library
(client + server + CLI), not a slim data crate. Under
`cfg(not(target_family = "wasm"))` it pulls in a mandatory, non-optional
`isahc` -> `curl` -> `curl-sys` + `openssl-sys` chain, plus `tokio`, `rusqlite`,
`notify`, `log4rs` and `interprocess`.

Consequences measured on this repo:

| build | needs libssl/libcurl | time |
| --- | --- | --- |
| `cargo test -p zclip-core` | no | ~0.9s |
| native build of `crates/zclip` | **yes** | ~60-70s |
| `cargo build --target wasm32-wasip1` | no | n/a |

`cargo tree --target wasm32-wasip1 -p zclip | grep -c 'openssl|curl|tokio|rusqlite'`
returns **0** — none of it is reachable on the real target. Splitting the crates
confines `zellij-tile` to the plugin crate, so the test loop stays hermetic and fast
and CI needs no `apt install libssl-dev`.

Rule that follows: **never run a bare `cargo test`/`cargo build`/`cargo clippy` at
the workspace root.** Scope with `-p zclip-core` or `--target wasm32-wasip1`.

## Technical notes
- Crate API reference: https://docs.rs/zellij-tile/latest/zellij_tile/
- `ZellijPlugin` trait signature and lifecycle: https://docs.rs/zellij-tile/latest/zellij_tile/trait.ZellijPlugin.html
  and https://zellij.dev/documentation/plugin-lifecycle.html
- `load(&mut self, configuration: BTreeMap<String,String>)` receives KDL-config-derived key/value pairs
- `update(&mut self, event: Event) -> bool` and `pipe(&mut self, pipe_message: PipeMessage) -> bool` should
  return `true` only when a re-render is required
- Follow the crate layout used by https://github.com/zellij-org/rust-plugin-example as a structural reference
- **`crates/zclip` must be a binary crate, NOT a `cdylib`.** This is counter-intuitive
  and was found only by running the plugin: Zellij loads plugins as WASI *command*
  modules and calls the module entry point before any export. A `cdylib` built for
  `wasm32-wasip1` exports `load`/`update`/`render`/`pipe` correctly but emits neither
  `_start` nor `_initialize`, so Zellij rejects it at load time with the opaque error
  `could not find exported function`. The build succeeds and CI stays green, so this
  cannot be caught without a live session. Use `[[bin]] name = "zclip", path = "src/main.rs"`;
  `register_plugin!` supplies `fn main()`, so `main.rs` must not define one.
- The workspace target dir is at the repo root, so the artifact path is `target/wasm32-wasip1/<profile>/zclip.wasm`

## Out of scope
- Permission requests and gating (separate issue)
- Any buffer/business logic

## Depends on
None
