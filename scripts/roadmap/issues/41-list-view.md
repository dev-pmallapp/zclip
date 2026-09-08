---
title: Render the buffer list view
milestone: M4 - Buffer Browser UI
labels: story,area:ui,priority:p0
---

## Context
The buffer list is the primary surface of zclip: without it, stored yanks are invisible and unusable. Users need to see, at a glance, what has been captured, in what order, and with enough preview text to recognize the buffer they want. This is the first rendering work built on top of the plugin's core render loop.

## Acceptance criteria
- [ ] `render(&mut self, rows: usize, cols: usize)` draws a scrollable list of stored buffers, most-recent-first
- [ ] Each row shows a truncated single-line preview of the buffer content, sized to `cols`
- [ ] A selection cursor is shown and moves with up/down key handling
- [ ] The list scrolls to keep the selected row visible when it exceeds `rows`
- [ ] Empty state (no buffers stored yet) renders a clear placeholder message
- [ ] Multi-line buffer content is flattened/truncated for the preview without crashing on unicode boundaries

## Technical notes
- Implement against `render(&mut self, rows: usize, cols: usize)` from `zellij-tile = "0.45"` (https://docs.rs/zellij-tile/latest/zellij_tile/); `rows`/`cols` are the authoritative pane dimensions for line count and truncation width.
- Use `print!`/`println!` with ANSI escapes for the list; consider `zellij-tile-utils` for styling helpers rather than hand-rolled escape codes.
- Keep row-building logic separate from ANSI styling so it can be reused by the fuzzy filter (#42) and responsive layout (#45) work.

## Out of scope
- Filtering/searching the list (separate story)
- Color/theme sourcing from the Zellij palette (separate story)

## Depends on
None
