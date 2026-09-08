---
title: EPIC: optional system clipboard bridge
milestone: M3 - System Clipboard Bridge
labels: epic,area:clipboard,priority:p0
---

## Context
zclip's in-app yank buffer (built in M2) is useful on its own, but many users want yanked text to also land in the OS clipboard so it can be pasted into other applications. Zellij provides a primary, permission-gated API (`copy_to_clipboard`) for this, plus lower-level building blocks (shell-out commands, OSC 52) for cases where more control or visibility into failures is needed. This epic covers making that bridge optional, configurable, and robust across platforms.

## Acceptance criteria
- [ ] Yanked text from copy mode can be pushed to the system clipboard via `copy_to_clipboard` when the feature is enabled.
- [ ] The plugin can detect or be told which clipboard backend (xclip, wl-copy, pbcopy, clip.exe) is appropriate for the current environment.
- [ ] A shell-out fallback path exists for environments/backends not covered by Zellij's own `copy_command` handling, with stdin-piping handled correctly (no native stdin support in `run_command`).
- [ ] Clipboard failures are surfaced to the user rather than silently swallowed.
- [ ] The OSC 52 fallback path is documented with verified (not assumed) behavior across common terminal emulators.
- [ ] The whole bridge is configurable and can be disabled entirely, falling back to yank-buffer-only behavior.

## Technical notes
- `zellij_tile::shim::copy_to_clipboard(text: impl Into<String>)` requires `WriteToClipboard` and respects the user's Zellij-level clipboard configuration. https://docs.rs/zellij-tile/latest/zellij_tile/shim/fn.copy_to_clipboard.html
- Zellij config keys relevant here: `copy_command`, `copy_clipboard` (`"system"` | `"primary"`), `copy_on_select`. https://zellij.dev/documentation/options.html
- `run_command` / `run_command_with_env_variables_and_cwd` (requires `RunCommands`) do not pipe stdin to the child process — this is a hard constraint the shell-out backend issue must solve explicitly.

## Out of scope
- Copy-mode selection and yank-buffer mechanics themselves (M2 epic).
- Clipboard history/multiple-buffer management beyond a single "last yank" bridge, unless a later milestone extends it.

## Child issues
- Primary path: bridge yanks through copy_to_clipboard
- Detect the platform clipboard backend
- Shell-out backends for xclip, wl-copy, pbcopy and clip.exe
- Surface clipboard failures to the user
- Document and verify the OSC 52 fallback path
- Clipboard configuration options
