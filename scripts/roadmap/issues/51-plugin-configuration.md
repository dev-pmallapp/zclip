---
title: Parse and hot-reload plugin configuration
milestone: M5 - Config, Keybindings & Pipes
labels: story,area:config,priority:p0
---

## Context
Users need to tune zclip without recompiling: buffer limits, whether system-clipboard bridging is enabled, which bridge binary to prefer, and similar knobs. Zellij passes configuration as plain key-value pairs from KDL, plugin aliases, or the CLI, and can push updates at runtime, so zclip needs a single typed config struct that both paths populate.

## Acceptance criteria
- [ ] `load(&mut self, configuration: BTreeMap<String,String>)` parses known keys (e.g. `buffer_limit`, `clipboard_bridge`) into a typed config struct with documented defaults
- [ ] Unknown keys are ignored without panicking, and a warning is surfaced (e.g. in a status line) rather than silently dropped
- [ ] Invalid values (e.g. non-numeric `buffer_limit`) fall back to the default and surface a warning instead of crashing
- [ ] `Event::PluginConfigurationChanged(BTreeMap<String,String>)` is handled and updates the live config struct without requiring a plugin restart
- [ ] Config can be supplied from a layout `pane { plugin location="..." { key "value" } }` block, from a plugin alias, and from `zellij action launch-or-focus-plugin --configuration "key=value"`, and all three are covered by a manual test note in the PR
- [ ] The full set of supported keys and defaults is documented in-repo (README or a CONFIG doc)

## Technical notes
- `load(&mut self, configuration: BTreeMap<String,String>)` and `Event::PluginConfigurationChanged(BTreeMap<String,String>)`. https://zellij.dev/documentation/plugin-api-configuration.html
- CLI form: `zellij action launch-or-focus-plugin --configuration "key=value"`.
- Keep the parsed config struct as the single source of truth consumed by the UI (M4) and pipe handling (#52).

## Out of scope
- A settings UI for editing configuration from within the plugin
- Validating clipboard bridge binaries actually exist on `$PATH` (covered by earlier clipboard-bridge milestones)

## Depends on
None
