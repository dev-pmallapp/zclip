---
title: Unit-test ring semantics on the native target
milestone: M1 - Core Yank Buffer Engine
labels: task,area:core,priority:p0
---

## Context
Since zellij-tile 0.45.0, host functions compile to no-ops on non-wasm targets,
which means `cargo test` can exercise the buffer ring logic natively without a
WASM toolchain, as long as the ring model stays decoupled from host calls. This
task delivers a thorough native test suite covering push/rotate/evict/name-lookup
semantics before those semantics are relied upon by yank/paste/persistence code.

## Acceptance criteria
- [ ] Tests cover pushing buffers below, at, and above the configured ring limit
- [ ] Tests cover eviction order (oldest unnamed buffer evicted first) when the ring is full
- [ ] Tests cover named buffers being excluded from eviction and correctly overwritten on re-use of a name
- [ ] Tests cover "most recent buffer" and "buffer at index N" accessors, including empty-ring edge cases
- [ ] Tests cover configuring the ring limit from a parsed `buffer_limit` string value, including invalid input
- [ ] `cargo test` runs and passes on the native host target with no WASM toolchain installed
- [ ] Tests are wired into the CI `test` job from "CI: fmt, clippy, test and wasm32-wasip1 build on every PR"

## Technical notes
- Rely on the zellij-tile 0.45+ guarantee that host functions no-op off-wasm, so these tests do not
  need any mocking layer for `zellij_tile`: https://docs.rs/zellij-tile/latest/zellij_tile/
- Place tests alongside the pure-logic module(s) from "Implement the PasteBuffer and bounded BufferRing
  data model" and "Support named buffers (tmux set-buffer / -b semantics)" using standard `#[cfg(test)]` modules

## Out of scope
- Integration tests that exercise a real Zellij session
- Persistence-layer tests (tracked with the persistence spike)

## Depends on
Implement the PasteBuffer and bounded BufferRing data model, Support named buffers (tmux set-buffer / -b semantics)
