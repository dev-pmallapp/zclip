---
title: Ship an example KDL config and plugin alias
milestone: M5 - Config, Keybindings & Pipes
labels: task,area:config,area:docs,priority:p0
---

## Context
Users copy-paste their way into new plugins. Without a ready-to-use KDL snippet covering the plugin alias, configuration keys, and a keybinding, adoption friction is high even if the plugin itself works correctly. This ships the canonical example referenced by the README (#61).

## Acceptance criteria
- [ ] Repo includes an example KDL file defining a `zclip` plugin alias pointing at a `file:` path, matching the plugin-aliases documentation shape
- [ ] The example sets at least one configuration key (e.g. `buffer_limit`) to demonstrate the config surface from #51
- [ ] The example includes a `bind` block using `LaunchOrFocusPlugin "zclip" { floating true; }` to demonstrate a working keybinding
- [ ] The example is validated by manually loading it into a local Zellij config and confirming the plugin launches
- [ ] The example file is referenced from the README installation section (#61)

## Technical notes
- Plugin alias shape:
  ```kdl
  plugins {
      zclip location="file:/path/to/zclip.wasm" {
          buffer_limit "50"
      }
  }
  ```
  https://zellij.dev/documentation/plugin-aliases.html
- Keybinding example: `bind "c" { LaunchOrFocusPlugin "zclip" { floating true; }; }`, noting the other supported args (`in_place`, `move_to_focused_tab`, `skip_plugin_cache`, `close_replaced_pane`) and that `LaunchPlugin` always spawns a new instance. https://zellij.dev/documentation/keybindings-possible-actions.html#launchorfocusplugin

## Out of scope
- A full user-facing config reference doc (tracked under #51/#61)
- Packaging the example as an installable template/scaffold tool

## Depends on
Parse and hot-reload plugin configuration
