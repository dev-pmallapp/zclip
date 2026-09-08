---
title: EPIC: v0.1.0 release readiness
milestone: M6 - v0.1.0 Release
labels: epic,area:docs,priority:p0
---

## Context
Shipping a first usable release means more than working code: users need documentation to install and configure zclip, an automated pipeline that publishes a built `.wasm` artifact, a clear statement of which Zellij versions are supported, a manual QA pass across the clipboard-bridge platforms zclip targets, and visibility in the community plugin index. This epic tracks everything needed to cut and announce v0.1.0.

## Acceptance criteria
- [ ] README documents installation, configuration, keybindings, and includes a demo
- [ ] A GitHub Actions workflow builds `zclip.wasm` for `wasm32-wasip1` and attaches it to tagged releases with a checksum
- [ ] A documented Zellij version compatibility matrix exists, with a runtime guard warning users on mismatch
- [ ] A manual QA checklist covering all supported clipboard-bridge platforms has been run and passed for the release candidate
- [ ] zclip is submitted to `awesome-zellij`
- [ ] v0.1.0 is tagged and the release workflow succeeds end-to-end
- [ ] All child issues in this milestone are closed

## Technical notes
- Distribution model: no plugin registry exists; plugins ship as a `.wasm` asset on a GitHub release, referenced via `file:` or `https://` URL in user KDL. Community index: https://github.com/zellij-org/awesome-zellij
- Version compatibility: plugin API version tracks the Zellij version, so `zellij-tile` must match the user's Zellij release; runtime check available via `get_zellij_version() -> String`. https://docs.rs/zellij-tile/latest/zellij_tile/shim/fn.get_zellij_version.html

## Out of scope
- Feature work belonging to M4/M5 (this epic assumes that functionality is complete)
- Post-v0.1.0 versioning/maintenance policy

## Child issues
- README: install, configure, keybindings and demo
- Automated release workflow publishing zclip.wasm
- Zellij version compatibility matrix and runtime guard
- Cross-platform manual QA checklist
- Submit zclip to awesome-zellij
