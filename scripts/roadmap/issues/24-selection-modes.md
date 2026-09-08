---
title: Char-wise, line-wise and block-wise selection
milestone: M2 - Copy Mode & Selection
labels: story,area:core,priority:p0
---

## Context
tmux copy mode supports three selection shapes: char-wise (arbitrary start/end within lines), line-wise (whole lines), and block-wise (rectangular column ranges). zclip needs the same to be a credible tmux replacement, since users routinely rely on block selection for tabular output and line selection for grabbing whole log lines.

## Acceptance criteria
- [ ] Entering selection from a cursor position sets an anchor; moving the cursor (via issue 23 motions) extends the selection live.
- [ ] Char-wise selection produces the exact substring between anchor and cursor, correctly handling multi-line spans.
- [ ] Line-wise selection produces whole lines between anchor and cursor, inclusive.
- [ ] Block-wise selection produces a rectangular slice bounded by the anchor and cursor columns across the spanned rows, independent of line length differences.
- [ ] A visible overlay highlights the current selection against the rendered pane content while in copy mode.
- [ ] A yank action extracts the selected text into zclip's internal buffer as a plain string and exits copy mode.
- [ ] Unit tests cover all three selection modes against a fixed sample buffer, including selections that span the viewport/history boundary.

## Technical notes
- Builds on the buffer model from issue 21 and cursor motion from issue 23; selection is computed purely against zclip's own line buffer, not via any Zellij "selected text" concept for pane content the user is currently selecting with a mouse (that is `PaneContents.selected_text` / `get_selected_text()`, relevant instead to issue 26's external-copy detection).
- No Zellij API models copy-mode selection shapes — this state machine is entirely zclip's responsibility.

## Out of scope
- Bridging the yanked text to the system clipboard (M3 epic).
- Multi-selection / disjoint selections (single contiguous selection only for this issue).

## Depends on
Read pane scrollback into a selectable buffer; Copy-mode entry, exit and key interception; Cursor motions and a vi/emacs key table
