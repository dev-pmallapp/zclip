---
title: EPIC: tmux-like copy mode
milestone: M2 - Copy Mode & Selection
labels: epic,area:core,priority:p0
---

## Context
zclip's core value proposition is a tmux-style copy mode: enter a mode, move a cursor over pane scrollback, select text, and yank it into an in-app buffer. Zellij does not ship this feature natively and exposes no "selection complete" event, so zclip must read pane contents itself, render its own selection overlay, and manage its own key interception. This epic tracks the work needed to get a usable, keyboard-driven copy mode shipped.

## Acceptance criteria
- [ ] A user can enter copy mode with a configurable keybinding and see a visual indication that copy mode is active.
- [ ] The user can move a cursor through the full scrollback (not just the visible viewport) and select text char-wise, line-wise, or block-wise.
- [ ] A yank action copies the selection into zclip's internal buffer and exits copy mode.
- [ ] Incremental search lets the user jump to a location in scrollback without leaving copy mode.
- [ ] All child issues in this milestone are closed and integration-tested together in a single plugin build.

## Technical notes
- Built on `zellij_tile = "0.45"`, targeting `wasm32-wasip1`. https://docs.rs/zellij-tile/latest/zellij_tile/
- Relies on `get_pane_scrollback`, `Event::Key` / `Event::InterceptedKeyPress`, and `Event::ModeUpdate` — see child issues for per-API detail and required permissions.

## Out of scope
- System clipboard bridging (tracked separately in the M3 clipboard epic).
- Mouse-driven selection; this epic is keyboard-first.

## Child issues
- Read pane scrollback into a selectable buffer
- Copy-mode entry, exit and key interception
- Cursor motions and a vi/emacs key table
- Char-wise, line-wise and block-wise selection
- Incremental search within scrollback
- Auto-yank text copied outside of copy mode
