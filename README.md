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

No configuration options exist yet (they arrive in M5). For now, load the plugin and bind a key to launch it:

```kdl
plugins {
    zclip location="file:~/.config/zellij/plugins/zclip.wasm"
}

keybinds {
    normal {
        bind "Ctrl y" {
            LaunchOrFocusPlugin "zclip" { floating true; }
        }
    }
}
```

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
