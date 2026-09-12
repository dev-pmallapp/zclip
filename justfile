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

# Mirrors what CI's `nix` job runs. Needs Nix with flakes; if you're not on
# Nix, ignore this, the flake is not mandatory.
nix-check:
    nix flake check --all-systems --no-build
    nix flake check --print-build-logs

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

# Reproduce the release workflow's artifact locally: release build,
# wasm-opt -Oz with the same flags CI uses, and a sha256. Handy for checking
# the actual shipped size before pushing a tag.
dist: build-release
    #!/usr/bin/env bash
    set -euo pipefail
    in=target/wasm32-wasip1/release/zclip.wasm
    if ! command -v wasm-opt >/dev/null 2>&1; then
        echo "error: wasm-opt not found on PATH (install Binaryen, e.g. \`nix-shell -p binaryen\`)" >&2
        echo "       skipping optimization; release CI requires it, this shell does not." >&2
        exit 1
    fi
    before=$(stat -c%s "$in")
    wasm-opt -Oz \
        --enable-bulk-memory \
        --enable-sign-ext \
        --enable-mutable-globals \
        --enable-nontrapping-float-to-int \
        "$in" -o zclip.wasm
    after=$(stat -c%s zclip.wasm)
    sha256sum zclip.wasm > zclip.wasm.sha256
    echo "size before wasm-opt: $before bytes"
    echo "size after wasm-opt:  $after bytes"
    cat zclip.wasm.sha256

clean:
    cargo clean
