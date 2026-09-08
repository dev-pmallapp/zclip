---
title: Clipboard configuration options
milestone: M3 - System Clipboard Bridge
labels: story,area:clipboard,area:config,priority:p1
---

## Context
Clipboard bridging touches multiple concerns — whether it's enabled at all, which backend to prefer, and how it relates to Zellij's own clipboard settings — and all of it needs a coherent configuration surface exposed through zclip's plugin config rather than scattered hard-coded choices. This issue defines that schema so issues 31-33 have a stable place to read settings from.

## Acceptance criteria
- [ ] A top-level `enabled`/`disabled` flag turns the entire clipboard bridge on or off, defaulting to a documented value.
- [ ] A `mode` (or similarly named) option lets the user choose between the `copy_to_clipboard` primary path (issue 31) and the direct shell-out path (issue 33), with a documented default.
- [ ] A `backend` override option lets the user force a specific backend (`xclip`, `xsel`, `wl-copy`, `pbcopy`, `clip.exe`) instead of relying on auto-detection (issue 32).
- [ ] Configuration is read from zclip's plugin configuration (documented location/format) at startup, with sane defaults if a key is absent.
- [ ] Invalid configuration values (e.g. an unrecognized backend name) produce a clear startup warning rather than a silent no-op or panic.
- [ ] Configuration options are documented in the project README/docs alongside how they interact with Zellij's own `copy_command`, `copy_clipboard`, and `copy_on_select` settings, so users understand the two layers of configuration.
- [ ] Unit tests cover parsing of valid and invalid configuration values.

## Technical notes
- Zellij's own relevant settings for context/interaction: `copy_command` (e.g. `"xclip -selection clipboard"`, `"wl-copy"`, `"pbcopy"`), `copy_clipboard` (`"system"` | `"primary"`), `copy_on_select` (default `true`). https://zellij.dev/documentation/options.html
- zclip's config schema is layered on top of, not a replacement for, these Zellij settings — when `mode` selects the primary path (issue 31), Zellij's own `copy_command`/`copy_clipboard` still govern the actual destination; when `mode` selects shell-out (issue 33), zclip's `backend` setting takes precedence instead.
- Coordinate naming/format with the vi/emacs key table config introduced in issue 23 so the overall plugin configuration file has one consistent schema style.

## Out of scope
- The actual detection algorithm (issue 32) and shell-out execution (issue 33) — this issue only defines and parses the configuration surface they consume.
- A GUI/TUI settings editor; configuration is file/KDL-based per Zellij plugin conventions.

## Depends on
None
