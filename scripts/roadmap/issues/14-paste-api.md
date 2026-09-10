---
title: Paste: write buffer contents into a target pane
milestone: M1 - Core Yank Buffer Engine
labels: story,area:core,priority:p0
---

## Context
Capturing yanks is only half the workflow; users need to paste a buffer's
contents back into a terminal pane, mirroring tmux `paste-buffer`. This story
implements the write-side of the engine, gated behind the `WriteToStdin`
permission.

## Acceptance criteria
- [ ] A paste action selects a buffer from the ring (most-recent by default, or by name/index)
- [ ] Selected buffer contents are written to the target pane via `write_chars_to_pane_id` / `write_to_pane_id`
- [ ] Paste is a no-op (with a rendered message) until `WriteToStdin` permission is granted
- [ ] Pasting into a nonexistent/closed pane fails gracefully without panicking
- [ ] Multi-line buffer contents are written faithfully (no dropped or reordered lines)
- [ ] Manual test: yank in one pane, paste into another pane in the same running Zellij session

## Technical notes
- Pane write APIs: `write_chars_to_pane_id` / `write_to_pane_id` (needs `WriteToStdin` permission)
- Permission gating must reuse the flow from "Implement the permission request and gating flow"
- Determining the target pane id may require correlating with `Event::PaneUpdate(PaneManifest)`
  from `ReadApplicationState`

## Out of scope
- System clipboard bridging (later milestone)
- Buffer selection UI (menu/picker) beyond a minimal default-to-most-recent behavior

## Post-implementation correction (M1 review)

The acceptance criterion "a paste action selects a buffer from the ring" was met,
but the trigger is wrong: paste is currently bound to a keypress handled by
`Event::Key`, which **only fires while the zclip pane itself is focused**. That
is backwards. tmux pastes into the pane you are working in, and requiring the
user to focus the clipboard plugin first defeats the purpose.

The fix is `MessagePlugin` in a keybinding, which reaches the plugin as
`pipe()` with `PipeSource::Keybind` and works while a terminal pane keeps
focus. Target resolution moves to `get_focused_pane_info()` rather than the
last-focused pane tracked from `PaneUpdate`.

Tracked in #47; the focused-only key handler is removed there rather than kept
as a second input path.

## Depends on
Implement the PasteBuffer and bounded BufferRing data model, Implement the permission request and gating flow
