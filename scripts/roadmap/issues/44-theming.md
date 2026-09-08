---
title: Theme the UI from the user's Zellij palette
milestone: M4 - Buffer Browser UI
labels: story,area:ui,priority:p1
---

## Context
Hardcoded colors clash with the wide variety of Zellij themes users run. Zellij exposes the active style/palette to plugins, and zclip should render its list, highlights, and status line using that palette so it feels native rather than bolted-on.

## Acceptance criteria
- [ ] The plugin requests the `ReadApplicationState` permission and subscribes to `Event::ModeUpdate(ModeInfo)`
- [ ] Colors for normal text, selection highlight, and fuzzy-match highlight are derived from the received palette instead of fixed ANSI codes
- [ ] The UI re-renders with updated colors when the user switches Zellij themes at runtime
- [ ] A sane fallback palette is used if no `ModeUpdate` has been received yet
- [ ] `Event::Visible(bool)` is handled so the plugin can skip unnecessary rendering/work while hidden

## Technical notes
- Palette/style data arrives via `Event::ModeUpdate(ModeInfo)`, which requires the `ReadApplicationState` permission; visibility toggling via `Event::Visible(bool)`. See https://docs.rs/zellij-tile/latest/zellij_tile/
- Confirm the exact shape of `ModeInfo`'s style/palette fields against the current `zellij-tile = "0.45"` docs before implementing; flag any discrepancy for follow-up.
- Centralize color lookups in one module so list rendering (#41), filter highlighting (#42), and status messages (#43) all pull from the same palette source.

## Out of scope
- User-configurable custom color overrides beyond the Zellij theme (could be a future config option)
- Light/dark auto-detection heuristics beyond what Zellij already provides

## Depends on
Render the buffer list view
