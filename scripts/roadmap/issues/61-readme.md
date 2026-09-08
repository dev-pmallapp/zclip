---
title: README: install, configure, keybindings and demo
milestone: M6 - v0.1.0 Release
labels: task,area:docs,priority:p0
---

## Context
The README is the first thing a prospective user sees, and for a plugin distributed as a bare `.wasm` file with no package manager, it is the entire onboarding experience. It needs to cover what zclip does, how to install it, how to configure and bind it, and show it working.

## Acceptance criteria
- [ ] README explains what zclip does (in-app yank/paste buffer, tmux-like browser, optional system-clipboard bridging) in the first few lines
- [ ] Installation section covers downloading the release `.wasm` and referencing it via `file:`/`https://` in KDL, per the plugin-aliases model
- [ ] Configuration section documents all keys from #51 with defaults
- [ ] Keybinding section shows a working `LaunchOrFocusPlugin` bind example, matching the shipped example KDL from #55
- [ ] A demo (recorded GIF/asciicast, or clearly described step-by-step walkthrough) shows browsing, filtering, and pasting a buffer
- [ ] A clipboard-bridge section explains xclip/wl-copy/pbcopy/clip.exe support and any per-platform caveats surfaced by the QA pass (#64)
- [ ] A compatibility note links to or summarizes the version matrix from #63

## Technical notes
- Reference the plugin-aliases documentation shape for install instructions: https://zellij.dev/documentation/plugin-aliases.html
- Reference `LaunchOrFocusPlugin`/`LaunchPlugin` semantics for the keybinding section: https://zellij.dev/documentation/keybindings-possible-actions.html#launchorfocusplugin
- Pull the example KDL directly from #55 rather than duplicating a second inline example that can drift out of sync.

## Out of scope
- A full auto-generated API/rustdoc site
- Translations into other languages

## Depends on
Ship an example KDL config and plugin alias
