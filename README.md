# zclip

[![CI](https://github.com/dev-pmallapp/zclip/actions/workflows/ci.yml/badge.svg)](https://github.com/dev-pmallapp/zclip/actions/workflows/ci.yml)

`zclip` is a [Zellij](https://zellij.dev) plugin that implements a tmux-like in-app yank-buffer (paste-buffer) ring, with optional bridging to the system clipboard through Zellij's own configured clipboard command.

## Status

Alpha software: the core yank/paste/copy-mode workflow works end to end, but
it is not yet release-ready and there is no tagged release to install.

What works today:

- A paste-buffer ring with eviction, plus named/pinned buffers that never get evicted
- Yank and paste, including pasting into whichever pane is currently focused
- Copy mode: scrollback reading, vi and emacs keymap presets, per-action `key_*` overrides, cursor motions, and char/line/block selection with reverse-video highlighting
- A buffer list view with previews and delete/yank/paste actions
- A pipe interface (`yank`, `paste`, `copy_mode`, `cancel`, `list`) usable from keybindings or `zellij pipe`, plus headless/background loading
- Opt-in, per-session persistence of the buffer ring (`persist "session"`), surviving plugin reloads and detach/attach but not the Zellij session ending — see [`docs/persistence.md`](docs/persistence.md)
- CI (fmt, clippy, tests, wasm build) and a tag-triggered release workflow

Notable gaps before v0.1.0:

- No incremental search in copy mode, and no observation of copies made outside zclip
- Clipboard integration only reaches Zellij's own configured clipboard command; there is no shell-out to `xclip`/`wl-copy`/`pbcopy`/`clip.exe` and no OSC 52 fallback
- No fuzzy filtering or theming in the buffer list
- No tagged release yet

## Requirements

- **Zellij 0.45 or newer.** This is a hard requirement, not a recommendation:
  the compiled plugin advertises the `zellij-tile` API version it was built
  against (currently 0.45.1), and older hosts reject the plugin outright rather
  than loading it with reduced functionality. Zellij 0.44.x will not work.
- Rust stable with the `wasm32-wasip1` target

## Install

There is no tagged release yet. Build from source:

```sh
cargo build --workspace --target wasm32-wasip1 --release
cp target/wasm32-wasip1/release/zclip.wasm ~/.config/zellij/plugins/zclip.wasm
```

Or use the Nix flake, which builds the same plugin plus the example config:

```sh
nix build github:dev-pmallapp/zclip
# -> result/bin/zclip.wasm
# -> result/share/zclip/zclip.kdl
```

`nix develop` also gives you a dev shell with the pinned toolchain if you'd
rather build from a checkout.

A downloadable `zclip.wasm` release artifact is planned for v0.1.0 (M6).

## Configuration

See [`examples/zclip.kdl`](examples/zclip.kdl) for a complete, commented example.
If you installed via the flake, that same file is installed alongside the
plugin at `$out/share/zclip/zclip.kdl` (the plugin itself is
`$out/bin/zclip.wasm`).

The essentials:

```kdl
plugins {
    zclip location="file:~/.config/zellij/plugins/zclip.wasm" {
        buffer_limit "50"
    }
}

load_plugins {
    zclip          // run in the background so paste works from any pane
}

keybinds {
    normal {
        bind "Alt y" { MessagePlugin "zclip" { name "copy_mode"; launch_new true; floating true; }; }
        bind "Alt q" { MessagePlugin "zclip" { name "cancel"; }; }
        bind "Alt p" { MessagePlugin "zclip" { name "paste"; launch_new true; }; }
        bind "Alt b" { MessagePlugin "zclip" { name "list"; floating true; launch_new true; }; }
    }
}
```

`Alt p` pastes into whichever pane you are currently in - you do not need to
focus zclip first. `buffer_limit` bounds *unnamed* buffers only; named buffers
are pinned and never evicted, as in tmux. `persist` (`off` by default,
`session` to opt in) keeps the buffer ring alive across plugin reloads and
detach/attach within a single Zellij session; see
[`docs/persistence.md`](docs/persistence.md) for exactly what that means and
its privacy tradeoffs before turning it on.

tmux's `prefix [` is not reproduced literally: in a terminal `Ctrl+[` *is* the
Escape byte, so binding it would break Escape.

## Roadmap

Work is tracked across GitHub milestones M0-M6 (49 tracked issues, 7 of them epics).

| Milestone | Description | Status |
| --- | --- | --- |
| M0 | Scaffold & CI | Complete |
| M1 | Core yank buffer engine (paste-buffer ring, named buffers, yank/paste, persistence) | Ring, yank/paste and persistence complete; named buffers exist in the engine but nothing creates one yet |
| M2 | Copy mode & selection (scrollback reading, key interception, motions, char/line/block selection, search) | Largely complete; search and observing external copies outstanding |
| M3 | System clipboard bridge (`copy_to_clipboard`, backend detection, shell-out backends, OSC 52 fallback) | Partial — only Zellij's own clipboard path is wired up |
| M4 | Buffer browser UI (list, fuzzy filter, actions, theming) | Partial — list view and actions exist; no filter or theming |
| M5 | Config, keybindings & pipes (`zellij pipe`, plugin-to-plugin messaging, headless mode) | Largely complete; plugin-to-plugin messaging outstanding |
| M6 | v0.1.0 release | Release workflow in place; nothing tagged yet |

## Development

See [`docs/dev.md`](docs/dev.md) for the full development guide, including the workspace layout and hermetic test setup.

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for workspace layout, build/test commands, and PR expectations.

## License

MIT
