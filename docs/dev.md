# Developing zclip

## Prerequisites

The easiest path is the flake in this repo:

```sh
nix develop
```

(or `direnv allow`, since a `.envrc` with `use flake` is committed). Either
gives you the pinned Rust toolchain with the `wasm32-wasip1` target already
installed, plus `just`, `cargo-watch`, `binaryen` and `zellij` — without
writing anything into `~/.rustup` or `~/.cargo`. It also carries
`pkg-config`, `openssl` and `curl` so that a *native* build of
`crates/zclip` (see below for why you'd ever want one) is debuggable from
inside the shell without leaving it.

The flake is not mandatory, though: plain rustup works fine on any
platform.

- Rust stable, with the `wasm32-wasip1` target:

  ```sh
  rustup target add wasm32-wasip1
  ```

  (`rust-toolchain.toml` already declares this target, so a plain `rustup`
  invocation in this repo picks it up automatically — the command above is
  only needed if rustup complains the target is missing.)

- `zellij` on `PATH`.
- Optionally `just` and `cargo-watch` for the fast dev loop.

**NixOS note (no flakes):** if you're on NixOS without flakes enabled,
`nix-shell -p rustup` followed by
`rustup toolchain install stable --target wasm32-wasip1` still works. Note
that this writes a second Rust toolchain into your home directory — exactly
what the flake above avoids, so prefer it if you can enable flakes.

## The workspace layout

The workspace has two crates:

- `crates/zclip-core` — pure Rust, zero dependencies, hermetic. All plugin
  logic should live here, with unit tests alongside it. It builds and tests
  natively in under a second.
- `crates/zclip` — the Zellij plugin. A **binary** crate, not a `cdylib`:
  Zellij loads plugins as WASI *command* modules and a `cdylib` emits no
  `_start`, so the host rejects it with "could not find exported function".
  It depends on `zellij-tile`,
  which transitively pulls in `zellij-utils` and (on non-wasm targets) a
  chain of isahc -> curl -> openssl-sys. This crate is host glue only: wire
  up Zellij's plugin API and delegate to `zclip-core`.

New logic goes in `zclip-core` with tests; `zclip` should stay a thin shim.

Buffer-ring persistence is one instance of that split: the codec
(`crates/zclip-core/src/persist.rs`) is unit-tested natively as part of
`zclip-core`'s hermetic suite, while the filesystem behaviour it enables —
actually reading and writing `/cache`, surviving reloads, GC — is host glue
in `crates/zclip` and is covered by a manual checklist instead. See
[`docs/persistence.md`](persistence.md).

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
just nix-check
```

`just nix-check` is what CI's `nix` job runs: it evaluates the flake for
every advertised system and then builds the `zclip` derivation for the
current one, catching Cargo.lock drift or a broken buildPhase/installPhase
that a plain `cargo build` wouldn't. It needs Nix with flakes enabled; skip
it if you're not on Nix.

**Never run a bare `cargo test` or `cargo build` at the workspace root.**
Doing so will try to link `crates/zclip` natively, pulling in
openssl/curl/tokio — roughly 70s and system library requirements you don't
need for wasm development. Always scope commands explicitly:

- `-p zclip-core` for native test/build/clippy.
- `--target wasm32-wasip1` for anything involving `crates/zclip`.

## Releasing

There's no plugin registry for Zellij, so a release is just a GitHub
release with a `zclip.wasm` and a checksum attached. To cut one:

1. Bump `workspace.package.version` in `Cargo.toml`.
2. Move the relevant `CHANGELOG.md` entries out of `[Unreleased]` into a new
   `## [x.y.z]` section.
3. Commit, then tag: `git tag vx.y.z` (the `v` prefix matters).
4. `git push origin vx.y.z`.

Pushing the tag triggers `.github/workflows/release.yml`, which builds the
plugin for `wasm32-wasip1`, runs it through `wasm-opt -Oz`, computes a
sha256, and attaches both `zclip.wasm` and `zclip.wasm.sha256` to a new
GitHub release for that tag.

The workflow's version guard re-derives the version from `Cargo.toml` and
fails the run if it doesn't match the pushed tag. This is deliberate: a tag
that disagrees with the crate version would produce an artifact that lies
about what it is, so the workflow refuses to publish rather than let that
happen silently.

Because `zclip.wasm` is a single portable WASM binary — it's the same
bytes regardless of the host OS or architecture — there is no per-OS/arch
build matrix and no separate downloads to pick between. One artifact covers
every platform Zellij itself runs on.

To exercise the whole pipeline (build, optimize, checksum, upload) without
actually publishing a release, run the workflow manually from the Actions
tab (`workflow_dispatch`) with `dry_run` left at its default of `true`. This
runs every step except creating the GitHub release, and still uploads the
built `zclip.wasm`/`zclip.wasm.sha256` as a workflow artifact so you can
download and inspect them.

`just dist` reproduces the release artifact locally (release build +
`wasm-opt -Oz` + sha256) if you want to check the shipped size before
tagging.
