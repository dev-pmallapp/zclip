---
title: Automated release workflow publishing zclip.wasm
milestone: M6 - v0.1.0 Release
labels: task,area:ci,priority:p0
---

## Context
Since there is no plugin registry, users install zclip by downloading a `.wasm` file from a GitHub release and pointing to it in their KDL. That artifact needs to be built reproducibly and attached automatically on tag pushes, with a checksum so users (and the compat-matrix tooling in #63) can verify what they downloaded.

## Acceptance criteria
- [x] A GitHub Actions workflow triggers on version tag pushes (e.g. `v*`)
- [x] The workflow builds the plugin with `cargo build --release --target wasm32-wasip1`
- [x] The workflow runs `wasm-opt` (or documents why it's skipped) to reduce binary size, since community plugins typically land in the 100-400 KB range and size matters when users load the plugin over HTTP
  - Measured on the real artifact: 832,469 -> 723,409 bytes with `wasm-opt -Oz`. Note this lands well above the 100-400 KB range this issue anticipated; `zellij-tile` plus its prost/protobuf machinery dominates, and shrinking it further is its own piece of work, not a release-pipeline concern.
  - `--enable-bulk-memory` is required: rustc lowers memcpy/memset to `memory.copy`/`memory.fill`, which wasm-opt rejects unless told to expect them. The other `--enable-*` flags are defensive against a future rustc emitting more.
- [x] The workflow computes a SHA256 checksum of the final `zclip.wasm` and attaches both the wasm and the checksum file to the GitHub release
- [ ] A dry-run (e.g. a manual `workflow_dispatch` or a test tag) has been executed and produces a downloadable, correctly checksummed artifact
  - **Still open.** The workflow has never executed on GitHub. Everything statically checkable was verified (YAML parses, the pinned Binaryen URL and `softprops/action-gh-release@v3` both resolve, the version guard's `awk` returns `0.1.0` against the real `Cargo.toml`, and `wasm-opt` was run locally against the actual release build), but a green run is the only thing that closes this. Trigger it from the Actions tab with `dry_run` left at its default of `true` before cutting the first tag.
- [x] The release process is documented (what triggers it, how to cut a release) for future maintainers — see the "Releasing" section in `docs/dev.md`.

## Technical notes
- Build target: `wasm32-wasip1`, per `zellij-tile = "0.45"`'s WASM plugin model. https://docs.rs/zellij-tile/latest/zellij_tile/
- `wasm-opt` is part of the Binaryen toolchain; confirm the exact install/invocation in CI (e.g. via a GitHub Action or apt/binary download) since this needs verification in the actual workflow implementation.
- Checksum step can use a standard `sha256sum` invocation in the workflow shell step.

## Out of scope
- Publishing to any crates.io-style registry (none exists for Zellij plugins)
- Signing releases with GPG or similar (can be a later hardening pass)

## Depends on
None
