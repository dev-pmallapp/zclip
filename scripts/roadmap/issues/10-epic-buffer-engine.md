---
title: EPIC: tmux-like paste-buffer ring
milestone: M1 - Core Yank Buffer Engine
labels: epic,area:core,priority:p0
---

## Context
zclip's core value proposition is an in-app yank/paste buffer modeled on tmux's
paste-buffer stack: a bounded ring of captured text, optionally named, that can be
pasted back into any pane and survives across plugin reloads. This epic covers the
data model, capture (yank) and playback (paste) APIs, named-buffer support,
persistence, and native unit tests, independent of any system-clipboard bridging.

## Acceptance criteria
- [x] A `PasteBuffer`/`BufferRing` data model exists with a configurable size bound
- [ ] Named buffers can be set and referenced (tmux `set-buffer -b <name>` equivalent)
      — half done: `BufferRing::push_named`/`set_name`/`clear_name` exist and `resolve`
      looks buffers up by name, but nothing in `crates/zclip` calls them, so no user
      path creates a named buffer yet. Naming from the list view is M4 (#31).
- [x] Yanking captured pane text pushes a new buffer onto the ring
- [x] Pasting writes the most-recent (or selected) buffer's contents into a target pane
- [x] The ring survives a plugin reload (persistence mechanism spiked and implemented)
- [x] Ring semantics (push, rotate, evict, name lookup) are covered by native unit tests
- [x] All logic here is decoupled from clipboard-bridge/system-clipboard concerns

## Technical notes
- Since zellij-tile 0.45.0, host functions no-op on non-wasm targets, so the ring model
  must be implemented as pure Rust logic with host calls kept at the edges, enabling
  `cargo test` on the native target: https://docs.rs/zellij-tile/latest/zellij_tile/
- Pane read/write uses `write_chars_to_pane_id` / `write_to_pane_id` (needs `WriteToStdin`)
  and `Event::PaneUpdate(PaneManifest)` (needs `ReadApplicationState`)

## Out of scope
- System clipboard bridging (xclip/wl-copy/pbcopy/clip.exe) — later milestone
- UI/keybinding design for triggering yank/paste (later milestone)

## Child issues
- Implement the PasteBuffer and bounded BufferRing data model
- Support named buffers (tmux set-buffer / -b semantics)
- Yank: capture selected text into the buffer ring
- Paste: write buffer contents into a target pane
- Persist the buffer ring across plugin reloads
- Unit-test ring semantics on the native target
