---
title: EPIC: Project scaffold, build toolchain and CI
milestone: M0 - Scaffold & CI
labels: epic,area:ci,priority:p0
---

## Context
zclip starts from an empty repository (only a LICENSE file). Before any yank/paste
logic can be written, we need a buildable Rust WASM plugin crate, a working
permission-request flow against the Zellij host, an automated CI pipeline, a fast
local dev loop, and baseline repo hygiene (README, contributing docs, issue/PR
templates). This epic tracks all scaffolding work required to get a green CI build
of a trivial, loadable Zellij plugin.

## Acceptance criteria
- [ ] `cargo build --target wasm32-wasip1` produces a loadable `.wasm` plugin binary
- [ ] The plugin registers via `register_plugin!` and implements the four `ZellijPlugin` lifecycle methods as no-op stubs
- [ ] The plugin requests and gates on the permissions it needs before calling any gated host API
- [ ] CI runs fmt, clippy, `cargo test` (native) and the wasm32-wasip1 build on every PR
- [ ] A documented dev loop exists for hot-reloading the plugin in a running Zellij session
- [ ] README skeleton, CONTRIBUTING, and issue/PR templates exist in the repo

## Technical notes
Pin `zellij-tile = "0.45"` and target `wasm32-wasip1` (formerly `wasm32-wasi`).
Reference the official example at https://github.com/zellij-org/rust-plugin-example
and the plugin dev environment docs at https://zellij.dev/documentation/plugin-dev-env.html.

## Out of scope
- Any yank/paste buffer logic (tracked under the M1 buffer-engine epic)
- System clipboard bridging (xclip/wl-copy/pbcopy/clip.exe) — a later milestone

## Child issues
- Bootstrap the Rust WASM plugin crate
- Implement the permission request and gating flow
- CI: fmt, clippy, test and wasm32-wasip1 build on every PR
- Developer hot-reload loop: dev layout and task runner
- Repo hygiene: README skeleton, CONTRIBUTING, issue and PR templates
