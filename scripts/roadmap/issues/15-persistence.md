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
- [ ] Spike: determine whether the plugin's WASI-mapped `/data` or `/cache` directory (or another
      mechanism) is writable/readable from within the plugin sandbox, and document findings
- [ ] Based on spike results, implement serialization of the `BufferRing` (e.g. via `serde` + JSON)
      to the chosen storage location
- [ ] On `load()`, the plugin attempts to deserialize and restore a prior ring before falling back to empty
- [ ] Corrupt or missing persisted state fails safe (starts with an empty ring, does not panic)
- [ ] Findings and the chosen mechanism are documented in `docs/` or code comments for future maintainers
- [ ] Persistence behavior is covered by at least one integration-style test or documented manual test steps

## Technical notes
- There is no confirmed, documented persistence API for Zellij plugins as of this writing.
  The `/data`/`/cache` WASI-mapped directories are a plausible candidate to investigate but
  this is UNVERIFIED — treat the first half of this issue as a spike, not an assumed API.
- If no viable in-sandbox persistence exists, document that as the spike outcome and consider
  narrowing scope to "in-memory only, survives hot-reload via `start-or-reload-plugin` state
  retention if any, otherwise explicitly not persisted" rather than inventing a mechanism.
- Do not block other M1 stories on this one; the ring model must work correctly with or without persistence.

## Out of scope
- Cross-session sync or cloud backup of buffers
- Persisting to the system clipboard (separate milestone)

## Depends on
Implement the PasteBuffer and bounded BufferRing data model
