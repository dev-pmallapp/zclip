---
title: EPIC: configuration, keybindings and CLI integration
milestone: M5 - Config, Keybindings & Pipes
labels: epic,area:config,priority:p0
---

## Context
A buffer browser is only useful if it's easy to invoke and configure. This epic covers loading and hot-reloading plugin configuration, exposing zclip's yank/paste/list/clear operations over `zellij pipe` so it can be scripted from the CLI or bound to keys, enabling plugin-to-plugin messaging, and supporting a headless/background mode that captures yanks without the UI open. It also covers shipping the example KDL configuration users copy into their own config.

## Acceptance criteria
- [ ] Plugin configuration is parsed from KDL/CLI key-value pairs on load and hot-reloaded at runtime
- [ ] `zellij pipe` commands can yank, paste, list, and clear zclip buffers from outside the UI
- [ ] Another plugin can talk to zclip via a documented plugin-to-plugin pipe surface
- [ ] zclip can run invisibly at startup via `load_plugins`, capturing yanks without the browser UI open
- [ ] An example KDL snippet and plugin alias ship in the repo for users to copy
- [ ] All child issues in this milestone are closed

## Technical notes
- Configuration: `load(&mut self, configuration: BTreeMap<String,String>)` and `Event::PluginConfigurationChanged(BTreeMap<String,String>)`. https://zellij.dev/documentation/plugin-api-configuration.html
- Plugin aliases: https://zellij.dev/documentation/plugin-aliases.html
- Keybindings: `LaunchOrFocusPlugin`/`LaunchPlugin` actions. https://zellij.dev/documentation/keybindings-possible-actions.html#launchorfocusplugin
- Pipes: `fn pipe(&mut self, pipe_message: PipeMessage) -> bool`, `PipeMessage`/`PipeSource`. https://zellij.dev/documentation/zellij-plugin-and-pipe.html and https://docs.rs/zellij-tile/latest/zellij_tile/prelude/struct.PipeMessage.html
- Headless loading via `load_plugins { ... }`. https://zellij.dev/documentation/plugin-loading.html

## Out of scope
- The buffer browser UI itself (covered by M4)
- Release packaging and distribution (covered by M6)

## Child issues
- Parse and hot-reload plugin configuration
- Drive zclip from the CLI via zellij pipe
- Expose a plugin-to-plugin messaging surface
- Run zclip as a background/headless plugin
- Ship an example KDL config and plugin alias
