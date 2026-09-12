---
title: Nix flake devshell for reproducible plugin development
milestone: M0 - Scaffold & CI
labels: task,area:ci,priority:p1
---

## Context
Building zclip needs a Rust toolchain carrying the `wasm32-wasip1` target, which
distro- and Nix-packaged `rustc` builds generally do not ship. On NixOS in
particular there is no `rustup` by default and the system `rustc` only has
`wasm32-unknown-unknown` and `wasm32v1-none` std available, so
`cargo build --target wasm32-wasip1` fails out of the box.

The current workaround, documented in `docs/dev.md`, is
`nix-shell -p rustup` followed by `rustup toolchain install stable --target wasm32-wasip1`.
That works, but it installs a second Rust toolchain into `~/.rustup` and puts
proxy shims in `~/.cargo/bin`, which can shadow or conflict with a
Nix-managed Rust. A flake devshell gives the right toolchain without touching
the user's home directory.

## Acceptance criteria
- [x] `flake.nix` exposes a `devShells.default` providing a Rust toolchain **with the `wasm32-wasip1` target**
- [x] The shell also provides `just`, `cargo-watch` and `zellij`
- [x] The shell provides `pkg-config`, `openssl` and `curl` so that a *native* build of `crates/zclip` works for anyone who needs to debug one (normally unnecessary — see #2)
- [x] `nix develop` followed by `just build` produces `target/wasm32-wasip1/debug/zclip.wasm` on a machine with no rustup (`just build-release` for the release profile; `just build` is the dev-loop recipe and is deliberately unoptimized)
- [x] `flake.lock` is committed
- [x] `nix flake check` passes
- [x] `docs/dev.md` leads with the flake and demotes the `nix-shell -p rustup` route to a fallback
- [x] A `.envrc` (`use flake`) is provided for direnv users, and `.direnv/` is gitignored

## Technical notes
- Use a toolchain overlay that can add targets — either
  [fenix](https://github.com/nix-community/fenix) or
  [rust-overlay](https://github.com/oxalica/rust-overlay). With rust-overlay:
  `rust-bin.stable.latest.default.override { targets = [ "wasm32-wasip1" ]; }`
- Pin `nixpkgs` to a channel whose `zellij` satisfies the minimum version the
  plugin requires (see #43 — `plugin_version()` makes this a hard floor, and
  nixpkgs stable has lagged at 0.44.x)
- Keep `rust-toolchain.toml` in the repo regardless: it is what makes rustup
  users outside Nix get the right target automatically
- Do not make the flake mandatory; contributors on other platforms should still
  be able to use plain rustup

## Out of scope
- Packaging zclip itself as a Nix derivation for end users (only the dev shell here)
- Building the plugin in CI via Nix — GitHub Actions keeps using `dtolnay/rust-toolchain` (#4)

## Depends on
Bootstrap the Rust WASM plugin crate
