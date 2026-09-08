---
title: Responsive layout for narrow and floating panes
milestone: M4 - Buffer Browser UI
labels: task,area:ui,priority:p2
---

## Context
zclip can be launched as a floating pane or a narrow tiled pane, and the render loop already receives the current pane size. The list, filter, and action UI need to degrade gracefully at small sizes instead of overflowing or truncating unreadably.

## Acceptance criteria
- [ ] At small `cols` values, previews truncate with an ellipsis instead of wrapping or overflowing the pane
- [ ] At small `rows` values, the status/help line is dropped before the list itself is cut short
- [ ] A minimum usable size is defined and, below it, a short "pane too small" message is shown instead of a broken layout
- [ ] Manual verification covers at least: default floating pane size, a half-width tiled pane, and a full-width tab pane
- [ ] No panics or index-out-of-bounds errors occur at `rows`/`cols` values of 1

## Technical notes
- Layout decisions are driven entirely by the `rows: usize, cols: usize` arguments to `render(&mut self, rows: usize, cols: usize)` (`zellij-tile = "0.45"`, https://docs.rs/zellij-tile/latest/zellij_tile/).
- Build this as a set of layout-breakpoint checks around the existing row-rendering helper from #41 rather than a separate rendering path.

## Out of scope
- Horizontal scrolling of long single-line previews
- Configurable layout breakpoints exposed to users

## Depends on
Render the buffer list view
