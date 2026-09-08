---
title: Implement the PasteBuffer and bounded BufferRing data model
milestone: M1 - Core Yank Buffer Engine
labels: story,area:core,priority:p0
---

## Context
Every other feature in the buffer-engine epic (yank, paste, naming, persistence)
depends on a shared, well-tested data structure. This story defines that
structure as pure Rust logic, independent of any Zellij host call, so it can be
exercised with native unit tests and reused unchanged by later stories.

## Acceptance criteria
- [ ] A `PasteBuffer` struct holds captured text plus metadata (creation order/index, optional name)
- [ ] A `BufferRing` struct holds an ordered collection of `PasteBuffer`s with a configurable maximum size
- [ ] Pushing a new buffer onto a full ring evicts the oldest unnamed buffer (tmux-like eviction)
- [ ] The ring exposes accessors for "most recent buffer" and "buffer at index N" (tmux `paste-buffer -b`-style addressing)
- [ ] The ring's max size is configurable and wired to a `buffer_limit` plugin config option
- [ ] The module has zero direct dependencies on `zellij_tile` host functions

## Technical notes
- Plugin config arrives as `BTreeMap<String,String>` in `load()`; parse a `buffer_limit` key from it
  and pass the parsed value into `BufferRing::new(limit)`
- Keep this module (e.g. `src/buffer.rs`) free of any `zellij_tile::prelude` host-call imports so it
  remains testable natively per the zellij-tile 0.45+ no-op behavior off-wasm:
  https://docs.rs/zellij-tile/latest/zellij_tile/

## Out of scope
- Named-buffer set/lookup API surface (separate story, though the struct field for name lives here)
- Persistence (serialization to disk)

## Depends on
Bootstrap the Rust WASM plugin crate
