# zclip dev tasks. Run `just` (or `just default`) to list them.

default:
    @just --list

# Build the whole workspace for the wasm32-wasip1 target (the only target
# the `zclip` cdylib actually links on).
build:
    cargo build --workspace --target wasm32-wasip1

# Same as `build` but optimized, matching the release profile in Cargo.toml.
build-release:
    cargo build --workspace --target wasm32-wasip1 --release

# Scoped to zclip-core: it's pure Rust with zero dependencies, so it builds
# and runs natively in <1s. `zclip` depends on zellij-tile, which pulls in
# zellij-utils -> isahc -> curl -> openssl-sys on non-wasm targets, so it
# must never be built/tested natively here.
test:
    cargo test -p zclip-core --all-targets

lint:
    cargo fmt --all -- --check
    cargo clippy -p zclip-core --all-targets -- -D warnings
    cargo clippy --workspace --target wasm32-wasip1 --all-targets -- -D warnings

fmt:
    cargo fmt --all

# Rebuild and hot-reload the plugin into the current Zellij session.
# The path must be absolute, hence $PWD.
dev: build
    zellij action start-or-reload-plugin file:$PWD/target/wasm32-wasip1/debug/zclip.wasm

# Load the plugin ad-hoc in a floating pane, useful when there's no existing
# plugin instance to reload. -s/--skip-plugin-cache is required, otherwise
# Zellij will happily serve a stale cached copy of the old binary.
run: build
    zellij plugin --floating -s -- file:{{justfile_directory()}}/target/wasm32-wasip1/debug/zclip.wasm

# Rebuild and reload on every change under crates/.
watch:
    cargo watch -w crates -s 'just dev'

# Open a dev session with the plugin already wired into a layout.
layout:
    zellij --layout dev/layout.kdl

clean:
    cargo clean
