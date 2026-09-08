# Developing zclip

## Prerequisites

- Rust stable, with the `wasm32-wasip1` target:

  ```sh
  rustup target add wasm32-wasip1
  ```

- `zellij` on `PATH`.
- Optionally `just` and `cargo-watch` for the fast dev loop.

**NixOS note:** this repo has no flake yet, so for the tooling above,
`nix-shell -p just cargo-watch` is enough. A *native* build of
`crates/zclip` additionally needs `nix-shell -p pkg-config openssl curl`,
because `zellij-tile` transitively pulls in curl/openssl on non-wasm
targets. In practice you should never need this: `zclip` is only ever built
for `wasm32-wasip1` (see below).

## The workspace layout

The workspace has two crates:

- `crates/zclip-core` — pure Rust, zero dependencies, hermetic. All plugin
  logic should live here, with unit tests alongside it. It builds and tests
  natively in under a second.
- `crates/zclip` — the `cdylib` Zellij plugin. It depends on `zellij-tile`,
  which transitively pulls in `zellij-utils` and (on non-wasm targets) a
  chain of isahc -> curl -> openssl-sys. This crate is host glue only: wire
  up Zellij's plugin API and delegate to `zclip-core`.

New logic goes in `zclip-core` with tests; `zclip` should stay a thin shim.

## The fast loop

```sh
just watch
```

Watches `crates/` and, on every change, rebuilds for `wasm32-wasip1` and
reloads the plugin into the current Zellij session (`just dev` under the
hood).

## Manual loop

If you'd rather not use `just`/`cargo-watch`, the fast loop is just two
commands:

```sh
cargo build --workspace --target wasm32-wasip1
zellij action start-or-reload-plugin file:$PWD/target/wasm32-wasip1/debug/zclip.wasm
```

## Ad-hoc floating load

To load the plugin without an existing instance to reload (e.g. no plugin
pane open yet):

```sh
zellij plugin --floating -s -- file:$PWD/target/wasm32-wasip1/debug/zclip.wasm
```

The `-s`/`--skip-plugin-cache` flag is important: without it, Zellij will
serve a cached copy of the old binary instead of picking up your rebuild.
This is the single most common dev-loop gotcha — if your changes don't
seem to show up, check that you passed `-s`.

## Using the dev layout

```sh
just layout
```

This runs `zellij --layout dev/layout.kdl`, which opens a terminal pane, the
zclip plugin pane, and the tab/status bars. It must be run from the repo
root, since the plugin path in the layout is relative. Run `just build`
first so the wasm artifact exists.

## Testing and linting

```sh
just test
just lint
```

**Never run a bare `cargo test` or `cargo build` at the workspace root.**
Doing so will try to link `crates/zclip` natively, pulling in
openssl/curl/tokio — roughly 70s and system library requirements you don't
need for wasm development. Always scope commands explicitly:

- `-p zclip-core` for native test/build/clippy.
- `--target wasm32-wasip1` for anything involving `crates/zclip`.
