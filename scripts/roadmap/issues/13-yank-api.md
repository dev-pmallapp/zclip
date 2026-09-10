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

## Post-implementation correction (M1 review)

**As implemented, this story does not deliver a usable feature.** `Zclip::yank`
reads `PaneContents::selected_text`, which the host only populates after a
**mouse drag**. There is no keyboard path to create a selection, so the tmux
workflow this project exists to reproduce is impossible with this issue alone.

The acceptance criteria above are all met; the criteria themselves were wrong.
"Read the relevant pane content" was specified without asking where a selection
comes from in a keyboard-driven workflow. The answer is copy mode (#16, #17,
#18), which owns cursor movement and selection and then calls into the ring.

Keep this issue's contribution -- ring push, blank rejection, permission gating
-- but treat mouse-selection yank as a secondary convenience path, not the
primary one. See #47 for the corrected keybinding contract.

## Depends on
Implement the PasteBuffer and bounded BufferRing data model, Implement the permission request and gating flow
