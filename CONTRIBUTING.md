# Contributing to zclip

Thanks for your interest in contributing. Work is tracked in GitHub milestones M0-M6; check the [issue tracker](https://github.com/dev-pmallapp/zclip/issues) and milestones for what's in scope before starting.

## Workspace layout

This is a Cargo workspace with two crates, split deliberately:

- `crates/zclip-core` - pure Rust, **zero dependencies**. All logic lives here: the yank buffer, copy mode state machine, clipboard backend selection, everything. Tests run natively in under a second on any machine, with no system libraries required.
- `crates/zclip` - the Zellij plugin, a **binary** crate (not a `cdylib` - see its `Cargo.toml`), depends on `zellij-tile = "0.45"`. This crate is a thin host shim only: it wires `zclip-core` to the Zellij plugin API and should contain as little logic as possible.

### Why the split

`zellij-tile` transitively pulls in `zellij-utils`, which on non-wasm targets requires `isahc -> curl -> curl-sys + openssl-sys`, plus `tokio`, `rusqlite`, and `notify`. Those dependencies are absent when targeting `wasm32-wasip1`, but a native `cargo build`/`cargo test` at the workspace root will pull them in and link against system OpenSSL/curl. Keeping all real logic in the dependency-free `zclip-core` crate keeps the test loop hermetic and roughly 70x faster.

**When adding code, ask: does this belong in `zclip-core` (logic, testable, no deps) or `crates/zclip` (host glue only)?** Default to `zclip-core`.

## Build, test, and lint

Use these exact commands:

```sh
cargo build --workspace --target wasm32-wasip1 --release   # -> target/wasm32-wasip1/release/zclip.wasm
cargo test -p zclip-core --all-targets                      # native tests
cargo fmt --all -- --check
cargo clippy -p zclip-core --all-targets -- -D warnings
cargo clippy --workspace --target wasm32-wasip1 --all-targets -- -D warnings
```

A `justfile` is also available: `just build`, `just test`, `just lint`, `just fmt`, `just dev`, `just run`, `just watch`, `just layout`, `just clean`.

See [`docs/dev.md`](docs/dev.md) for the full development guide.

### Hard rule: never run bare `cargo test` / `cargo build` / `cargo clippy` at the workspace root

A bare `cargo build`, `cargo test`, or `cargo clippy` at the workspace root will link `crates/zclip` natively and drag in openssl/curl. Always scope your command with `-p zclip-core` or `--target wasm32-wasip1`.

## Commit and PR expectations

- `cargo fmt --all -- --check`, `cargo clippy -p zclip-core --all-targets -- -D warnings`, and `cargo test -p zclip-core --all-targets` must all pass before you open a PR.
- Reference the relevant issue number in your PR description (e.g. `Closes #12`).
- Keep `zclip-core` dependency-free. Do not add a `[dependencies]` entry to that crate without discussing it in the issue first.
- Keep PRs scoped to a single issue/milestone item where possible.
