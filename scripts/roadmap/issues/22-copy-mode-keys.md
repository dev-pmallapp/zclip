---
title: Copy-mode entry, exit and key interception
milestone: M2 - Copy Mode & Selection
labels: story,area:core,priority:p0
---

## Context
Copy mode needs a clean lifecycle: a keybinding to enter it, a way to intercept subsequent keystrokes so they drive cursor/selection instead of reaching the underlying terminal pane, and an unambiguous exit (Escape, yank-and-exit, or explicit cancel). This is the control-flow backbone that issues 23-25 plug motions and selection logic into.

## Acceptance criteria
- [ ] A configured keybinding switches the plugin into copy-mode state and, if required, calls `switch_to_input_mode` to change Zellij's active input mode.
- [ ] While in copy mode, key presses are captured via `intercept_key_presses` / `Event::InterceptedKeyPress(KeyWithModifier)` rather than falling through to the focused terminal pane.
- [ ] The plugin correctly requests and handles the `InterceptInput` permission, including the denied/unavailable case.
- [ ] `Event::ModeUpdate(ModeInfo)` is consumed (requires `ReadApplicationState`) so zclip's internal state stays in sync if the user changes Zellij's mode through another path.
- [ ] Escape (or a configurable key) cleanly exits copy mode, releases key interception, and restores normal pane input.
- [ ] A state machine (enum-based) models at least: Normal, CopyModeIdle, CopyModeSelecting, and transitions between them are unit-testable without a running Zellij instance.

## Technical notes
- `Event::Key(KeyWithModifier)` fires for keys on the plugin's own pane and needs no permission; `intercept_key_presses` + `Event::InterceptedKeyPress(KeyWithModifier)` is required to grab keys while another pane is focused, and needs the `InterceptInput` permission.
- `switch_to_input_mode` requires `ChangeApplicationState`.
- `Event::ModeUpdate(ModeInfo)` requires `ReadApplicationState`.
- Zellij has no built-in copy-mode concept, so this state machine is entirely zclip-owned; do not assume any Zellij-side "mode" maps 1:1 onto it.

## Out of scope
- The actual motion/selection logic once a key is intercepted (issue 23, issue 24).
- Rendering the copy-mode overlay (covered alongside issue 24).

## Rendering constraint (verified)

A Zellij plugin **cannot draw over a terminal pane**. tmux enters copy mode
*in place*; zclip cannot. Copy mode must therefore render a *copy* of the
captured scrollback inside zclip's own (ideally full-screen floating) pane, and
the cursor the user moves is zclip's, not the terminal's.

This is close to indistinguishable full-screen, but it has real consequences:
the captured text is a snapshot, so a pane that keeps producing output will
drift from what is displayed. Decide explicitly whether to re-fetch via
`Event::PaneRenderReport` or freeze on entry, and say so in the UI.

`intercept_key_presses()` (`shim.rs:2775`) and `Event::InterceptedKeyPress`
(`data.rs:1019`) are confirmed to exist, so grabbing keys while another pane is
focused is possible -- but it does not solve rendering, which is the binding
constraint here.

## Depends on
Read pane scrollback into a selectable buffer
