---
title: Yank: capture selected text into the buffer ring
milestone: M1 - Core Yank Buffer Engine
labels: story,area:core,priority:p0
---

## Context
Yanking is the entry point that populates the buffer ring with real content from
the terminal. Without it, the ring model has nothing to hold. This story wires
pane content capture (gated behind the appropriate permission) into a new
`PasteBuffer` pushed onto the ring.

## Acceptance criteria
- [ ] The plugin subscribes to `Event::PaneUpdate(PaneManifest)` to know which pane/selection is active
- [ ] A yank action reads the relevant pane content and constructs a new `PasteBuffer`
- [ ] The new buffer is pushed onto the shared `BufferRing`, honoring eviction rules from the ring model
- [ ] Yanking is a no-op (with a rendered message) until `ReadApplicationState`/`ReadPaneContents` permission is granted
- [ ] An empty or whitespace-only selection does not create a new buffer entry
- [ ] Manual test: triggering yank in a running Zellij session adds an entry visible in the plugin's render output

## Technical notes
- Getting pane info: `Event::PaneUpdate(PaneManifest)` (needs `ReadApplicationState`)
- Permission gating must reuse the flow from "Implement the permission request and gating flow"
  (`request_permission`, `EventType::PermissionRequestResult`, `PermissionStatus::Granted`)
- Relevant permission docs: https://zellij.dev/documentation/plugin-api-permissions.html

## Out of scope
- Paste (separate story)
- System clipboard bridging on yank (later milestone)

## Depends on
Implement the PasteBuffer and bounded BufferRing data model, Implement the permission request and gating flow
