---
title: Primary path: bridge yanks through copy_to_clipboard
milestone: M3 - System Clipboard Bridge
labels: story,area:clipboard,priority:p0
---

## Context
Zellij exposes a single, permission-gated shim call, `copy_to_clipboard`, that hands text off to whatever clipboard destination the user has configured at the Zellij level. This should be zclip's default and preferred bridging path, since it requires no platform detection or shelling out on zclip's part — Zellij and its own `copy_command`/`copy_clipboard` settings do that work.

## Acceptance criteria
- [ ] When the clipboard bridge is enabled (see issue 36), every successful yank (from copy mode, issue 24) also calls `zellij_tile::shim::copy_to_clipboard` with the yanked text.
- [ ] The plugin declares and requests the `WriteToClipboard` permission, and handles denial without crashing (yank still succeeds locally, bridge is simply skipped with a visible notice).
- [ ] `Event::CopyToClipboard(CopyDestination)` is consumed to confirm the copy succeeded and to record which destination (`System | Primary | Command`) was used, for diagnostics.
- [ ] `Event::SystemClipboardFailure` is consumed and routed to the failure-surfacing mechanism built in issue 34.
- [ ] Behavior is verified against at least one configuration where `copy_command` is set and one where it is unset (OSC 52 fallback), documenting observed differences.
- [ ] Integration test or manual test script documents the exact Zellij config used for verification.

## Technical notes
- `zellij_tile::shim::copy_to_clipboard(text: impl Into<String>) ` — requires `WriteToClipboard`. https://docs.rs/zellij-tile/latest/zellij_tile/shim/fn.copy_to_clipboard.html
- This call respects whatever `copy_command`/`copy_clipboard` the user has configured in Zellij's own config; zclip does not need to reimplement backend selection for this path (contrast with issue 32/33, which cover the shell-out fallback for cases this path doesn't reach, e.g. when zclip wants a backend independent of Zellij's own config).
- `CopyDestination` variants: `System | Primary | Command`.

## Out of scope
- Backend detection and shell-out execution (issues 32, 33) — this issue only covers the direct shim path.
- Failure UI/UX details beyond routing the event (issue 34 owns presentation).

## Depends on
Char-wise, line-wise and block-wise selection; Clipboard configuration options
