---
title: Buffer actions: paste, delete, rename and pin
milestone: M4 - Buffer Browser UI
labels: story,area:ui,priority:p0
---

## Context
Viewing buffers is only half the workflow; users need to act on the selected entry. Paste is the primary action (matching tmux's paste-buffer behavior), while delete, rename, and pin let users curate the store so useful snippets don't get evicted or lost among noise.

## Acceptance criteria
- [ ] Pressing the paste key on a selected row injects that buffer's content into the previously focused pane and closes/hides the zclip UI
- [ ] Pressing the delete key removes the selected buffer from the store and re-renders the list
- [ ] Pressing the rename key enters an inline text-edit mode for a user-facing label on the buffer
- [ ] Pressing the pin key marks a buffer as pinned, exempting it from LRU/size-based eviction and sorting it to the top
- [ ] Unpinning restores normal sort/eviction behavior for that buffer
- [ ] All actions provide a brief on-screen confirmation or status line update
- [ ] Actions have no effect and show a no-op message when the list is empty

## Technical notes
- "Inject content into the previously focused pane" depends on whichever core paste mechanism was established in earlier milestones (e.g. writing to the terminal via existing zellij-tile input/write APIs); confirm the exact API in the core buffer milestone before wiring this up, and flag if it needs further verification against `zellij-tile` docs.
- Pin/rename state should live alongside the buffer struct in the core store; this issue only needs the UI-triggered mutations and re-render, not new storage design.
- Hiding the UI after paste can use `hide_self()` per https://zellij.dev/documentation/plugin-loading.html.

## Out of scope
- Multi-select / bulk actions across several buffers at once
- Undo history for deletes

## Depends on
Render the buffer list view
