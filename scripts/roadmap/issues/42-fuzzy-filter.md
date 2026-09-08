---
title: Fuzzy filter across stored buffers
milestone: M4 - Buffer Browser UI
labels: story,area:ui,priority:p1
---

## Context
As the buffer store grows, scrolling through a flat list becomes slow. A tmux-copy-mode-like fuzzy filter lets users type a few characters and jump straight to the buffer they want, which is the main productivity win of a browsable clipboard over a single-slot one.

## Acceptance criteria
- [ ] A keypress enters filter/search mode and captures subsequent characters as a query string
- [ ] The list view (#41) re-renders showing only buffers whose content fuzzy-matches the query
- [ ] Matching is case-insensitive and tolerant of non-contiguous character matches (fuzzy, not exact substring)
- [ ] Matched characters are visually highlighted in the preview text
- [ ] Backspace edits the query and re-filters live
- [ ] Escaping/clearing the query restores the full unfiltered list
- [ ] Filtering does not noticeably lag with realistic buffer counts (document the tested count in the PR)

## Technical notes
- This is pure Rust logic operating on the in-memory buffer store; no new zellij-tile API surface beyond the existing `render`/key-handling loop is required.
- Evaluate existing fuzzy-matching crates (e.g. a `skim`/`fzf`-style algorithm) vs. a small hand-rolled subsequence matcher; note the tradeoff (binary size vs. match quality) in the PR since this is a WASM plugin where size matters (see #62).
- Reuse the row-rendering helper introduced in #41 rather than duplicating truncation/preview logic.

## Out of scope
- Regex or glob-based search modes
- Persisting the last-used query across plugin restarts

## Depends on
Render the buffer list view
