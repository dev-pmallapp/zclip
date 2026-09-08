---
title: Cursor motions and a vi/emacs key table
milestone: M2 - Copy Mode & Selection
labels: story,area:core,priority:p1
---

## Context
Once copy mode is entered and keys are intercepted (issue 22), zclip needs a cursor that can move through the scrollback buffer (issue 21) using familiar keybindings. tmux users expect vi-style (`hjkl`, `w`/`b`, `0`/`$`, `gg`/`G`) or emacs-style (`C-f`/`C-b`/`C-n`/`C-p`) motions, so the key table should be pluggable rather than hard-coded to one style.

## Acceptance criteria
- [ ] A cursor position (line index + column) is tracked against the scrollback buffer model from issue 21 and can move up/down/left/right, word-forward/word-backward, and to start/end of line.
- [ ] A "jump to top of history" and "jump to bottom of viewport" motion are implemented (`gg`/`G` equivalents).
- [ ] A configurable key table supports at least a vi-style and an emacs-style preset, selectable via plugin configuration.
- [ ] Cursor motion clamps correctly at buffer boundaries (top of history, bottom of viewport) without panicking or wrapping unexpectedly.
- [ ] The key table is data-driven (e.g. a map from `KeyWithModifier` to a motion enum) so new bindings can be added without touching dispatch logic.
- [ ] Unit tests cover each motion against a fixed sample buffer, including boundary conditions.

## Technical notes
- Consumes `Event::InterceptedKeyPress(KeyWithModifier)` delivered per issue 22's interception setup.
- Motion should operate purely on the in-memory buffer built in issue 21; it does not need further Zellij shim calls.
- Key table format and defaults should be documented in plugin config (coordinate naming with issue 36 clipboard config so the overall config schema stays consistent).

## Out of scope
- Selection highlighting/state (issue 24 consumes cursor position but owns selection anchors).
- Search-driven cursor jumps (issue 25 is a separate motion source).

## Depends on
Read pane scrollback into a selectable buffer; Copy-mode entry, exit and key interception
