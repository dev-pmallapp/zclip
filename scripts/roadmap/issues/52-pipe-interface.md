---
title: Drive zclip from the CLI via zellij pipe
milestone: M5 - Config, Keybindings & Pipes
labels: story,area:config,priority:p1
---

## Context
`zellij pipe` lets external scripts and keybindings interact with a running plugin without opening its UI. Exposing zclip's core operations this way turns it into a scriptable clipboard manager, letting users yank from a shell command, paste a specific buffer by name, list buffer metadata for a status bar, or clear the store from a script.

## Acceptance criteria
- [ ] `fn pipe(&mut self, pipe_message: PipeMessage) -> bool` is implemented and dispatches on `pipe_message.name`
- [ ] `yank` command stores `pipe_message.payload` as a new buffer
- [ ] `paste` command selects a buffer via `pipe_message.args` (e.g. by index, id, or "most recent") and returns/injects its content
- [ ] `list` command returns buffer metadata (id, label, preview, pinned state) for consumption by scripts or other plugins
- [ ] `clear` command empties the buffer store (respecting pinned buffers unless explicitly overridden)
- [ ] A `None` payload correctly signals end-of-pipe and is handled without error
- [ ] The `ReadCliPipes` permission is requested and documented as required for CLI-sourced pipes
- [ ] Example `zellij pipe --plugin file:/path/to/zclip.wasm --name yank -- "text"` usage is documented

## Technical notes
- `fn pipe(&mut self, pipe_message: PipeMessage) -> bool`; `PipeMessage { source: PipeSource, name: String, payload: Option<String>, args: BTreeMap<String,String>, is_private: bool }`; `PipeSource` is `Cli(String) | Plugin(u32) | Keybind`. https://zellij.dev/documentation/zellij-plugin-and-pipe.html and https://docs.rs/zellij-tile/latest/zellij_tile/prelude/struct.PipeMessage.html
- CLI invocation: `zellij pipe --plugin file:/path/to/zclip.wasm --name yank -- "text"`; broadcast form `zellij pipe --name broadcast -- data` reaches all plugins and should be considered when deciding whether zclip should react to unnamed/broadcast pipes.
- Reading CLI-sourced pipes requires the `ReadCliPipes` permission.
- Design commands around `yank`/`paste`/`list`/`clear` as the named surface; keep parsing of `args` isolated so #53 can reuse it for plugin-to-plugin calls.

## Out of scope
- Plugin-to-plugin specific transport details (separate story, #53)
- Interactive/streaming pipe responses beyond a single reply payload

## Depends on
Parse and hot-reload plugin configuration
