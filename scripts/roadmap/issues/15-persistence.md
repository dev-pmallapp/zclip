---
title: Persist the buffer ring across plugin reloads
milestone: M1 - Core Yank Buffer Engine
labels: story,area:core,priority:p1
---

## Context
Zellij plugins can be reloaded (e.g. via `start-or-reload-plugin` during
development, or on Zellij restart), and losing the buffer ring on every reload
would be a poor user experience compared to tmux, whose paste buffers persist for
the life of the server. There is no documented, confirmed persistence API for
Zellij plugins, so this story starts as a spike to establish what is actually
available before implementing a chosen approach.

## Acceptance criteria
- [x] Spike: determine whether the plugin's WASI-mapped `/data` or `/cache` directory (or another
      mechanism) is writable/readable from within the plugin sandbox, and document findings
- [x] Based on spike results, implement serialization of the `BufferRing` (not via `serde` + JSON —
      see Technical notes) to the chosen storage location
- [x] On `load()`, the plugin attempts to deserialize and restore a prior ring before falling back to empty
- [x] Corrupt or missing persisted state fails safe (starts with an empty ring, does not panic)
- [x] Findings and the chosen mechanism are documented in `docs/` or code comments for future maintainers
- [x] Persistence behavior is covered by at least one integration-style test or documented manual test steps

## Technical notes
- Spike resolved: `/cache` is writable and survives a plugin reload; `/data` is not usable for
  this feature at all — Zellij's own `unload_plugin` (which a reload runs as unload-then-load)
  unconditionally `remove_dir_all`s the plugin's `/data` directory, and it additionally sits under
  a per-server-process UUID so it could not outlive a session either. Full source citations in
  `docs/persistence.md`'s "Spike findings" section.
- Serialization is NOT via `serde` + JSON as this issue originally suggested. `zclip-core` is
  dependency-free by design (see `CONTRIBUTING.md`), so pulling in `serde` was never on the table.
  Just as importantly, a derived `Deserialize` would reconstruct the ring's fields directly from
  the file, bypassing `enforce_limit`, name-uniqueness, the blank-text rule, and id/seq
  monotonicity — every invariant this module maintains. Restoring a ring has to go *through* the
  ring's own logic (`BufferRing::restore`), not around it via a field-for-field deserialize.
  Instead, `zclip-core::persist` defines a small bespoke text format; see `docs/persistence.md`
  for the spec.
- Do not block other M1 stories on this one; the ring model must work correctly with or without persistence.

## Out of scope
- Cross-session sync or cloud backup of buffers
- Persisting to the system clipboard (separate milestone)

## Depends on
Implement the PasteBuffer and bounded BufferRing data model
