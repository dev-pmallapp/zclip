# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial workspace scaffold with `zclip-core` and `zclip` crates.
- Permission gating for the Zellij plugin.
- CI pipeline.
- Paste-buffer ring with eviction, plus named/pinned buffers that never get evicted.
- Yank and paste, with paste writing into whichever pane is currently focused.
- Copy mode: scrollback reading, cursor motions, and char/line/block selection with reverse-video highlighting.
- Configurable copy-mode keymap, with vi and emacs presets and per-action `key_*` overrides.
- A report of copy-mode keys shadowed by the user's own Zellij keybinds.
- A buffer list view with previews and delete/yank/paste actions.
- A pipe interface (`yank`, `paste`, `copy_mode`, `cancel`, `list`) usable from keybindings and `zellij pipe`.
- Headless/background loading via `load_plugins`.
- Plugin configuration: `buffer_limit`, `keymap`, and per-action `key_*` overrides.
- An example KDL configuration (`examples/zclip.kdl`).
- Opt-in, per-session persistence of the buffer ring (`persist "session"`), surviving plugin reloads and detach/attach but not the end of the Zellij session.
- A Nix flake providing a package and a devShell.
- A tag-triggered release workflow that builds, optimizes, and publishes `zclip.wasm`.
