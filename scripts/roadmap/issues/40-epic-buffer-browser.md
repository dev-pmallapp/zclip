---
title: EPIC: interactive buffer browser
milestone: M4 - Buffer Browser UI
labels: epic,area:ui,priority:p0
---

## Context
zclip's core value is letting users browse and reuse everything they've yanked, tmux-style, without leaving the terminal. This epic covers the interactive UI surface: rendering the stored buffers, filtering them, acting on them, and making that surface look and behave correctly across themes and pane sizes. Everything here builds on the in-app buffer store delivered in earlier milestones and is rendered inside a Zellij plugin pane via `render(&mut self, rows: usize, cols: usize)`.

## Acceptance criteria
- [ ] A user can open the zclip pane and see a list of stored buffers with a preview of their contents
- [ ] A user can narrow the list with a fuzzy filter as they type
- [ ] A user can paste, delete, rename, and pin a buffer from the list without leaving the plugin
- [ ] The UI adopts the user's Zellij color palette instead of hardcoded ANSI colors
- [ ] The layout remains usable in narrow floating panes and full-width tab panes alike
- [ ] All child issues in this milestone are closed

## Technical notes
- Rendering entry point is `render(&mut self, rows: usize, cols: usize)` from `zellij-tile = "0.45"`, writing ANSI-styled text via `print!`/`println!`. See https://docs.rs/zellij-tile/latest/zellij_tile/
- Styling helpers live in `zellij-tile-utils`.
- Palette/theme data arrives via `Event::ModeUpdate(ModeInfo)` (requires the `ReadApplicationState` permission); visibility toggling via `Event::Visible(bool)`.

## Out of scope
- Clipboard capture and storage mechanics (covered by earlier core/clipboard milestones)
- Configuration and keybinding wiring (covered by M5)

## Child issues
- Render the buffer list view
- Fuzzy filter across stored buffers
- Buffer actions: paste, delete, rename and pin
- Theme the UI from the user's Zellij palette
- Responsive layout for narrow and floating panes
