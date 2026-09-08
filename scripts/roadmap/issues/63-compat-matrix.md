---
title: Zellij version compatibility matrix and runtime guard
milestone: M6 - v0.1.0 Release
labels: task,area:ci,area:docs,priority:p1
---

## Context
The plugin API version tracks the Zellij version itself, meaning `zellij-tile` must be matched to the user's installed Zellij release or the plugin may fail in confusing ways. Users need a documented support matrix, and the plugin should detect a mismatch at runtime and warn rather than fail silently or crash.

## Acceptance criteria
- [ ] A documented table lists which Zellij release(s) each zclip release was built and tested against
- [ ] The plugin calls `get_zellij_version() -> String` at startup and compares it against the known-compatible range
- [ ] On a detected mismatch, the plugin surfaces a clear, non-fatal warning (e.g. in the UI status line) rather than silently misbehaving
- [ ] CI (or a documented manual process) re-validates the matrix against new Zellij releases as they ship
- [ ] The matrix is linked from the README (#61)

## Technical notes
- Runtime version check: `get_zellij_version() -> String`. https://docs.rs/zellij-tile/latest/zellij_tile/shim/fn.get_zellij_version.html
- The exact semantics of `zellij-tile`'s version pinning vs. Zellij's runtime API version need verification against current `zellij-tile = "0.45"` release notes before finalizing the comparison logic; document any assumptions made.

## Out of scope
- Supporting multiple `zellij-tile` API versions from a single compiled plugin binary
- Automated CI matrix testing against every historical Zellij release (start with the current stable + the minimum supported version)

## Depends on
None
