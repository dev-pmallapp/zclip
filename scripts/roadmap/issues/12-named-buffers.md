---
title: Support named buffers (tmux set-buffer / -b semantics)
milestone: M1 - Core Yank Buffer Engine
labels: story,area:core,priority:p1
---

## Context
tmux users rely on naming a buffer (`set-buffer -b <name>`) to pin content they
want to reference later regardless of ring rotation. zclip should offer the same
affordance so users can protect specific yanks from eviction and paste them back
by name instead of by recency.

## Acceptance criteria
- [ ] A buffer can be assigned a user-supplied name at yank time or after the fact
- [ ] Named buffers are excluded from automatic eviction when the ring is full
- [ ] Looking up a buffer by name returns its contents, or a clear "not found" result
- [ ] Assigning an existing name to a new buffer replaces the prior buffer with that name (tmux `set-buffer -b` overwrite semantics)
- [ ] Unnamed buffers continue to rotate/evict normally alongside named ones
- [ ] Behavior is covered by native unit tests (paired with issue 16)

## Technical notes
- Extend the `PasteBuffer`/`BufferRing` model from "Implement the PasteBuffer and bounded BufferRing
  data model" rather than introducing a parallel structure
- Keep name-lookup as a pure in-memory map (name -> ring index or buffer id); no host API involved

## Out of scope
- UI/keybinding for prompting the user for a buffer name
- Persistence of names across reloads (covered by the persistence story, but this story defines the in-memory contract it must preserve)

## Depends on
Implement the PasteBuffer and bounded BufferRing data model
