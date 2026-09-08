---
title: Document and verify the OSC 52 fallback path
milestone: M3 - System Clipboard Bridge
labels: task,area:clipboard,area:docs,priority:p2
---

## Context
When no `copy_command` is configured in Zellij, `copy_to_clipboard` falls back to emitting an OSC 52 escape sequence (`\e]52;c;BASE64`) to the host terminal, which the terminal emulator itself is responsible for interpreting as a clipboard write. Support for this varies by terminal emulator and by intermediate multiplexers (e.g. tmux-in-between) or SSH sessions, so this path needs to be verified empirically rather than assumed to work uniformly, and the results documented for users.

## Acceptance criteria
- [ ] Verify OSC 52 write behavior in Alacritty and document the result (works / doesn't work / needs config).
- [ ] Verify OSC 52 write behavior in kitty and document the result.
- [ ] Verify OSC 52 write behavior in WezTerm and document the result.
- [ ] Verify OSC 52 behavior when Zellij is itself running inside tmux (tmux-in-between) and document any passthrough requirements.
- [ ] Verify OSC 52 behavior over an SSH session and document any caveats (e.g. terminal must support it locally, not just remotely).
- [ ] Explicitly document that OSC 52 clipboard *read-back* is gated behind Zellij's `dangerously_enable_paste_buffer_read` (default `false`) and is therefore not a generally available path — do not build any zclip feature that assumes read-back works.
- [ ] Findings are written up in project docs (e.g. a `docs/clipboard.md` or README section) referenced by issue 36's configuration docs.

## Technical notes
- OSC 52 sequence format: `\e]52;c;BASE64` written by Zellij to the host terminal when no `copy_command` is configured.
- `dangerously_enable_paste_buffer_read` (default `false`) gates OSC 52 read-back; treat read-back as unavailable unless a user has explicitly and knowingly enabled this. https://zellij.dev/documentation/options.html
- This task is verification-and-documentation only — do not add new code paths, since OSC 52 emission itself is handled by Zellij via `copy_to_clipboard` (issue 31), not by zclip directly.

## Out of scope
- Implementing any zclip-side OSC 52 emission code (Zellij already does this via `copy_to_clipboard`).
- Any read-back feature relying on `dangerously_enable_paste_buffer_read`.

## Depends on
Primary path: bridge yanks through copy_to_clipboard
