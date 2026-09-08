---
title: Read pane scrollback into a selectable buffer
milestone: M2 - Copy Mode & Selection
labels: story,area:core,priority:p0
---

## Context
Before zclip can render a selection or move a cursor, it needs a local, addressable model of the target pane's text: the current viewport plus history above and below it. This is the foundational data layer every other copy-mode feature (motion, selection, search) builds on.

## Acceptance criteria
- [ ] zclip requests scrollback for the focused pane via `get_pane_scrollback` and stores `viewport`, `lines_above_viewport`, and `lines_below_viewport` in a single addressable line buffer with stable line indices.
- [ ] The plugin declares and requests the `ReadPaneContents` permission and handles the permission-denied case gracefully (visible error, no panic).
- [ ] A full-scrollback fetch (`get_full_scrollback: true`) works for panes with large history without exceeding reasonable plugin memory/CPU budgets (document the measured limits).
- [ ] The buffer model exposes a stable line/column addressing scheme usable by later motion and selection code (issue 23, issue 24).
- [ ] Unit tests cover buffer construction from mocked `PaneContents` for empty pane, viewport-only, and full-history cases.
- [ ] `PaneRenderReport` events are optionally consumed to keep the buffer fresh while copy mode is open, with a documented decision on polling vs. one-shot fetch.

## Technical notes
- `zellij_tile::shim::get_pane_scrollback(pane_id: PaneId, get_full_scrollback: bool) -> Result<PaneContents, String>`. https://docs.rs/zellij-tile/latest/zellij_tile/shim/fn.get_pane_scrollback.html
- `PaneContents` fields: `viewport: Vec<String>`, `lines_above_viewport: Vec<String>`, `lines_below_viewport: Vec<String>`, `selected_text: Option<SelectedText>`, `cursor: Option<(usize, usize)>`, plus `get_selected_text() -> Option<String>`. https://docs.rs/zellij-tile/latest/zellij_tile/prelude/struct.PaneContents.html
- `Event::PaneRenderReport(HashMap<PaneId, PaneContents>)` delivers periodic updates and also requires `ReadPaneContents`; decide whether to use it for live-refresh or rely solely on the one-shot call made on copy-mode entry.
- There is no plugin-level `dump_screen`; `get_pane_scrollback` is the only programmatic read path — do not build around `DumpScreen` (that's a keybinding action only).

## Out of scope
- Rendering the buffer on screen (issue 22/24 handle overlay rendering).
- Multi-pane scrollback aggregation; this issue targets a single focused pane at a time.

## Depends on
None
