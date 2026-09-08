---
title: Detect the platform clipboard backend
milestone: M3 - System Clipboard Bridge
labels: story,area:clipboard,priority:p1
---

## Context
The shell-out fallback (issue 33) needs to know which clipboard command to invoke on the current platform. Rather than requiring users to hand-configure this in every environment, zclip should detect a sensible default from environment variables and OS signals, while still allowing explicit override (issue 36).

## Acceptance criteria
- [ ] Detection returns `wl-copy` when `WAYLAND_DISPLAY` is set.
- [ ] Detection returns `xclip` (or configured X11 tool) when `DISPLAY` is set and Wayland is not detected.
- [ ] Detection returns `pbcopy` on macOS.
- [ ] Detection returns `clip.exe` under WSL, identified via `/proc/version` containing "microsoft" or the `WSL_DISTRO_NAME` environment variable being set.
- [ ] Detection order/precedence is documented (e.g. Wayland checked before X11, WSL checked before generic Linux).
- [ ] When no backend can be detected, the plugin falls back to the `copy_to_clipboard` primary path (issue 31) or OSC 52 rather than erroring out, and logs which fallback was chosen.
- [ ] Unit tests cover each detection branch by injecting mock environment values (no reliance on the real host environment during CI).

## Technical notes
- Signals to check, per the verified facts for this project: `WAYLAND_DISPLAY` env var -> wl-copy; `DISPLAY` env var -> xclip/xsel; macOS (target OS) -> pbcopy; WSL via `/proc/version` containing "microsoft" or `WSL_DISTRO_NAME` set -> clip.exe.
- This detection result feeds directly into issue 33's shell-out backend selection.
- Detection logic runs inside the wasm32-wasip1 plugin sandbox — verify which of these environment/filesystem reads (`/proc/version`) are actually available to a Zellij plugin at runtime; document any WASI sandboxing limitation encountered instead of assuming parity with a native Rust binary.

## Out of scope
- Actually invoking the detected backend (issue 33).
- Detecting availability of the binary on `PATH` (may be a follow-up if detection alone proves insufficient).

## Depends on
Clipboard configuration options
