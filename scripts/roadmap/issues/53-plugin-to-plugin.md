---
title: Expose a plugin-to-plugin messaging surface
milestone: M5 - Config, Keybindings & Pipes
labels: story,area:config,priority:p2
---

## Context
Beyond CLI scripting, other Zellij plugins may want to push text into zclip's buffer store or query it directly (e.g. a status-bar plugin showing the most recent yank, or a workflow plugin chaining actions). Zellij supports plugin-to-plugin pipes via `pipe_message_to_plugin`, and zclip should accept and respond to these using the same named-command surface as the CLI pipe interface.

## Acceptance criteria
- [ ] Incoming `PipeMessage` with `source: PipeSource::Plugin(u32)` is distinguished from CLI/keybind sources and handled without requiring `ReadCliPipes`
- [ ] The same `yank`/`paste`/`list`/`clear` command names from #52 work when invoked by another plugin, avoiding a second parallel command set
- [ ] A minimal example (in docs or a test plugin) demonstrates calling zclip via `pipe_message_to_plugin(plugin_url, pipe_message)` from another plugin
- [ ] `is_private` messages are respected (not echoed or logged in a way that leaks payload contents)
- [ ] The messaging surface (command names, expected args, response shape) is documented for third-party plugin authors

## Technical notes
- Plugin-to-plugin transport uses `pipe_message_to_plugin(plugin_url, pipe_message)`; receiving side is the same `fn pipe(&mut self, pipe_message: PipeMessage) -> bool` as CLI pipes, distinguished via `PipeSource::Plugin(u32)`. https://zellij.dev/documentation/zellij-plugin-and-pipe.html and https://docs.rs/zellij-tile/latest/zellij_tile/prelude/struct.PipeMessage.html
- Reuse the command dispatch table built in #52; this issue is primarily about source-based routing/permissions and documentation, not a new protocol.

## Out of scope
- Authentication/authorization between plugins beyond what Zellij's pipe model already provides
- A stable versioned wire protocol for third-party integrations (document as best-effort until there's real external demand)

## Depends on
Drive zclip from the CLI via zellij pipe
