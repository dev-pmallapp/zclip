# zclip

[![CI](https://github.com/dev-pmallapp/zclip/actions/workflows/ci.yml/badge.svg)](https://github.com/dev-pmallapp/zclip/actions/workflows/ci.yml)

`zclip` is a [Zellij](https://zellij.dev) plugin that implements a tmux-like in-app yank-buffer (paste-buffer) ring, with optional bridging to the system clipboard via `xclip`, `wl-copy`, `pbcopy`, or `clip.exe`.

## Status

**Pre-alpha. Nothing works yet.** Only the M0 scaffold exists: the crate builds, requests permissions, and renders a placeholder line. There is no yank buffer, no copy mode, and no clipboard bridge yet. Do not expect a usable plugin at this stage.

## Requirements

- **Zellij 0.45 or newer.** This is a hard requirement, not a recommendation:
  the compiled plugin advertises the `zellij-tile` API version it was built
  against (currently 0.45.1), and older hosts reject the plugin outright rather
  than loading it with reduced functionality. Zellij 0.44.x will not work.
- Rust stable with the `wasm32-wasip1` target

## Install

There is no release yet. Build from source:

```sh
cargo build --workspace --target wasm32-wasip1 --release
cp target/wasm32-wasip1/release/zclip.wasm ~/.config/zellij/plugins/zclip.wasm
```

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
are pinned and never evicted, as in tmux.

tmux's `prefix [` is not reproduced literally: in a terminal `Ctrl+[` *is* the
Escape byte, so binding it would break Escape.

## Roadmap

Work is tracked across GitHub milestones M0-M6 (45 tracked issues).

| Milestone | Description |
| --- | --- |
| M0 | Scaffold & CI *(in progress)* |
| M1 | Core yank buffer engine (paste-buffer ring, named buffers, yank/paste, persistence) |
| M2 | Copy mode & selection (scrollback reading, key interception, motions, char/line/block selection, search) |
| M3 | System clipboard bridge (`copy_to_clipboard`, backend detection, shell-out backends, OSC 52 fallback) |
| M4 | Buffer browser UI (list, fuzzy filter, actions, theming) |
| M5 | Config, keybindings & pipes (`zellij pipe`, plugin-to-plugin messaging, headless mode) |
| M6 | v0.1.0 release |

## Development

See [`docs/dev.md`](docs/dev.md) for the full development guide, including the workspace layout and hermetic test setup.

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for workspace layout, build/test commands, and PR expectations.

## License

MIT
