---
title: Auto-yank text copied outside of copy mode
milestone: M2 - Copy Mode & Selection
labels: story,area:core,priority:p2
---

## Context
Users frequently mouse-select and copy text directly in a terminal pane without ever entering zclip's copy mode. tmux-like workflows benefit from zclip automatically picking up such copies into its internal buffer so the yank history stays complete. This is inherently limited by what Zellij actually exposes and needs a spike before committing to an implementation.

## Acceptance criteria
- [ ] Spike: confirm experimentally whether `Event::CopyToClipboard(CopyDestination)` fires reliably on mouse-driven copy-on-select, and document findings (including which `CopyDestination` variants — `System | Primary | Command` — are observed in practice).
- [ ] Because `CopyToClipboard` is a notification only and does not carry the copied text, implement a fallback that reacts to the event by calling `get_pane_scrollback` on the relevant pane and reading `PaneContents.selected_text` / `get_selected_text()` to recover the actual text.
- [ ] Handle the race condition where the selection may have been cleared by the time the follow-up scrollback read happens; document the observed failure rate.
- [ ] Recovered text is appended to zclip's internal yank buffer using the same storage path as manual copy-mode yanks (issue 24).
- [ ] The feature is behind a configuration flag (default off or on, to be decided during the spike) so users who find it noisy or unreliable can disable it.
- [ ] Required permissions (`ReadApplicationState` for the event, `ReadPaneContents` for the follow-up read) are declared and failure/denial is handled gracefully.

## Technical notes
- `Event::CopyToClipboard(CopyDestination)` fires when a copy succeeds anywhere in Zellij; `CopyDestination` is `System | Primary | Command`. It requires `ReadApplicationState`. It carries no text payload — this is a hard constraint, not an oversight to work around cleverly.
- `Event::SystemClipboardFailure` (also `ReadApplicationState`) is the failure counterpart; not directly needed here but related (see issue 34).
- Recovery path: `zellij_tile::shim::get_pane_scrollback` -> `PaneContents.selected_text` / `get_selected_text()`. https://docs.rs/zellij-tile/latest/zellij_tile/shim/fn.get_pane_scrollback.html
- Given the notification-only nature of the event and possible timing races, treat this issue as spike-first: land a documented proof-of-concept before committing to a permanent design.

## Out of scope
- Guaranteeing 100% capture of every external copy; document known gaps instead.
- Any change to how Zellij itself performs copy-on-select (`copy_on_select` config is out of zclip's control, see issue 36).

## Depends on
Read pane scrollback into a selectable buffer; Char-wise, line-wise and block-wise selection
