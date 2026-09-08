---
title: Cross-platform manual QA checklist
milestone: M6 - v0.1.0 Release
labels: task,area:docs,priority:p0
---

## Context
zclip's system-clipboard bridging spans several incompatible platform tools (xclip, wl-copy, pbcopy, clip.exe) plus edge cases like SSH sessions and nested tmux, where OSC 52 clipboard behavior differs. Automated CI cannot exercise real clipboard hardware/compositors, so a documented manual QA checklist is required before every release to catch regressions.

## Acceptance criteria
- [ ] A checklist document exists covering: Linux/X11 (xclip), Linux/Wayland (wl-copy), macOS (pbcopy), and WSL (clip.exe)
- [ ] The checklist includes an over-SSH scenario verifying clipboard bridging behavior (or documented lack thereof) when the terminal is remote
- [ ] The checklist includes a nested-tmux scenario verifying OSC 52 passthrough behavior when zclip runs inside Zellij inside tmux
- [ ] Each checklist item states the expected result and how to verify it (e.g. "paste into an external app and confirm text matches")
- [ ] The checklist has been executed at least once for the v0.1.0 release candidate, with results (pass/fail/platform notes) recorded
- [ ] Any platform-specific caveats discovered are fed back into the README's clipboard-bridge section (#61)

## Technical notes
- Target bridge tools per platform: xclip (Linux/X11), wl-copy (Linux/Wayland), pbcopy (macOS), clip.exe (WSL).
- OSC 52 behavior differs across terminal emulators and multiplexer nesting (Zellij inside tmux, or either over SSH); note in the checklist that results may be terminal-emulator-dependent and should be recorded per emulator tested.

## Out of scope
- Automating these checks in CI (no CI runners available for all these environments)
- Testing every terminal emulator; cover a representative sample and note gaps

## Depends on
None
