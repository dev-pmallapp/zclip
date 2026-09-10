---
title: Run zclip as a background/headless plugin
milestone: M2 - Copy Mode & Selection
labels: story,area:config,priority:p1
---

## Context
Some users want zclip capturing yanks continuously without a visible pane taking up screen space, only surfacing the browser UI when explicitly summoned. Zellij supports loading plugins invisibly at startup via a `load_plugins` config block, and plugins can hide/show themselves programmatically, which is the right shape for zclip's daemon-like capture behavior.

## Acceptance criteria
- [ ] zclip functions correctly when started via a `load_plugins { ... }` block with no visible pane
- [ ] The plugin calls `hide_self()` on startup when launched in background mode and `show_self()` when summoned via keybinding/pipe
- [ ] Buffer capture (yank events) continues to function while hidden, verified by pasting a captured buffer after re-opening the UI
- [ ] Permission prompts (if any are triggered by headless loading) are documented so users aren't surprised on first launch
- [ ] Documentation explains the difference between headless-loaded zclip and a normal `LaunchOrFocusPlugin`-invoked instance, including that they are the same plugin instance when addressed correctly

## Technical notes
- Headless loading via `load_plugins { ... }` in the Zellij config; plugins can call `hide_self()` / `show_self()`. https://zellij.dev/documentation/plugin-loading.html
- Verify whether a headless-loaded instance and a keybinding-launched instance are the same plugin instance or separate ones in current Zellij; this needs confirmation against Zellij's plugin loading behavior before finalizing the UX, and should be called out explicitly if unclear at implementation time.
- Combine with the pipe interface (#52) so `yank` pipe calls work identically whether zclip is visible or hidden.

## Out of scope
- Multi-instance synchronization if headless and UI instances turn out to be genuinely separate plugin instances (track as a follow-up if discovered)
- Auto-starting zclip for users who haven't opted into `load_plugins` (purely opt-in via user config)

## Depends on
Drive zclip from the CLI via zellij pipe
