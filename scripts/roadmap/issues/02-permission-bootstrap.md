---
title: Implement the permission request and gating flow
milestone: M0 - Scaffold & CI
labels: task,area:core,priority:p0
---

## Context
zclip needs to read pane contents, write to pane stdin, and eventually shell out to
system clipboard tools. Zellij plugins must explicitly request these capabilities
and wait for host confirmation before calling gated APIs. Getting this flow right
early avoids silent no-ops or panics once real yank/paste logic is added.

## Acceptance criteria
- [ ] `load()` calls `request_permission(&[...])` with the initial set of permissions the plugin needs
- [ ] The plugin subscribes to `EventType::PermissionRequestResult`
- [ ] `update()` matches on `Event::PermissionRequestResult(PermissionStatus::Granted)` and flips an internal "ready" flag
- [ ] Gated host calls (pane read/write) are only invoked after the ready flag is set
- [ ] A denied/pending permission state renders a clear message instead of silently doing nothing
- [ ] Manual test: loading the plugin in Zellij shows the OS permission prompt and the plugin becomes active after acceptance

## Technical notes
- Permission API and lifecycle: https://zellij.dev/documentation/plugin-api-permissions.html
- Relevant `PermissionType` variants to request now or reserve for later milestones:
  `ReadPaneContents`, `WriteToClipboard`, `RunCommands`, `ReadApplicationState`,
  `ChangeApplicationState`, `WriteToStdin`, `InterceptInput`, `ReadCliPipes`
- For this milestone, request at minimum `ReadApplicationState` and `WriteToStdin`;
  `RunCommands` and `WriteToClipboard` can be deferred to the clipboard-bridging milestone
  but the gating pattern established here must be reusable for them
- Store permission state as an enum (e.g. `Pending`, `Granted`, `Denied`) rather than a bool,
  to allow rendering distinct UI per state

## Out of scope
- Requesting clipboard-bridge-specific permissions (`RunCommands`, `WriteToClipboard`) end-to-end — only the reusable gating mechanism is in scope here
- Actual pane read/write logic

## Depends on
Bootstrap the Rust WASM plugin crate
