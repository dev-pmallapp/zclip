---
title: Incremental search within scrollback
milestone: M2 - Copy Mode & Selection
labels: story,area:core,priority:p1
---

## Context
tmux copy mode lets users search scrollback with `/` (forward) and `?` (backward) and jump the cursor to matches without leaving copy mode. This is essential for navigating long scrollback buffers efficiently rather than scrolling line by line, and it is a commonly requested feature for any copy-mode implementation.

## Acceptance criteria
- [ ] Pressing a configured key (default `/`) opens a search prompt within copy mode and captures typed characters via intercepted key events.
- [ ] Search matches update incrementally as the query is typed, highlighting the nearest match in the rendered buffer.
- [ ] `n`/`N` (or configured equivalents) jump to the next/previous match, moving the cursor from issue 23's model.
- [ ] Backward search (`?`) is supported in addition to forward search (`/`).
- [ ] Search wraps around the buffer boundary with a visible indicator when it does so.
- [ ] Canceling search (Escape) restores the cursor to its pre-search position.
- [ ] Unit tests cover forward/backward search, wraparound, and no-match cases against a fixed sample buffer.

## Technical notes
- Operates on the in-memory line buffer from issue 21; no additional Zellij shim calls are required for search itself.
- Reuses the key interception plumbing from issue 22 to capture the search query text.
- Match highlighting shares the overlay rendering path used for selection in issue 24 — coordinate rendering code to avoid duplicating the highlight logic.

## Out of scope
- Regex search (start with literal substring search; note regex as a possible future enhancement).
- Persisting search history across copy-mode sessions.

## Depends on
Read pane scrollback into a selectable buffer; Copy-mode entry, exit and key interception; Cursor motions and a vi/emacs key table
