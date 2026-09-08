---
title: Developer hot-reload loop: dev layout and task runner
milestone: M0 - Scaffold & CI
labels: task,area:ci,priority:p1
---

## Context
Iterating on a Zellij plugin by hand (build, locate the wasm artifact, reload)
is slow and error-prone. A documented, scriptable dev loop lets contributors
rebuild and reload the plugin in a running Zellij session in a single command,
which is essential for productive work on the yank/paste engine.

## Acceptance criteria
- [ ] A `dev` layout KDL file loads zclip from `file:target/wasm32-wasip1/debug/zclip.wasm`
- [ ] A task runner target (e.g. `just dev` or a shell script) builds the plugin and runs
      `zellij action start-or-reload-plugin file:target/wasm32-wasip1/debug/zclip.wasm`
- [ ] A documented ad-hoc load path exists using `zellij plugin --floating file:target/wasm32-wasip1/debug/zclip.wasm -s`
- [ ] Watching the crate for changes (e.g. via `cargo watch` or `entr`) triggers an automatic rebuild+reload
- [ ] Instructions for the dev loop are written in the README or a `docs/dev.md` file

## Technical notes
- Dev loop commands: `cargo build --target wasm32-wasip1` then
  `zellij action start-or-reload-plugin file:target/wasm32-wasip1/debug/zclip.wasm`
- Ad-hoc floating load: `zellij plugin --floating file:target/wasm32-wasip1/debug/zclip.wasm -s`
  (`-s` = `--skip-plugin-cache`, required to pick up rebuilt binaries)
- Reference dev environment docs: https://zellij.dev/documentation/plugin-dev-env.html
  and the example repo https://github.com/zellij-org/rust-plugin-example

## Out of scope
- CI pipeline itself (tracked separately)
- Packaging/release tooling

## Depends on
Bootstrap the Rust WASM plugin crate
