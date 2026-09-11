---
title: Preserve pane colours in copy mode
milestone: M2 - Copy Mode & Selection
labels: story,area:core,area:plugin,priority:p2
---

## Context

Entering copy mode replaces the target terminal pane with zclip's own pane,
and zclip re-renders the captured scrollback as plain text. All colour and
styling from the original pane is lost the moment copy mode opens; it only
reappears when copy mode exits and the real pane is restored underneath.

tmux does not have this problem: its `copy-mode` keeps the pane's own
colours on screen and draws only the *selection* in reverse video. Users
notice zclip's monochrome copy mode immediately and report it as a
regression relative to tmux.

## The upstream blocker

All of the following is verified against zellij 0.45.1 source, not assumed:

1. `get_pane_scrollback` is the only scrollback API available to plugins.
   `PluginCommand::GetPaneScrollback { pane_id, get_full_scrollback }`
   (`zellij-utils/src/data.rs:3534`) carries no ansi flag, and the handler at
   `zellij-server/src/screen.rs:9196` calls `pane.pane_contents(...)` — the
   colour-stripped variant. There is no styled counterpart reachable from
   this command.
2. The styled variant exists in the server and works fine:
   `pane_contents_with_ansi` (`zellij-server/src/panes/terminal_pane.rs:1290`).
   It is collected into `PaneRenderReport.all_pane_contents_with_ansi`
   (`zellij-utils/src/data.rs:2454`), but
   `WasmBridge::handle_pane_render_report`
   (`zellij-server/src/plugins/wasm_bridge.rs:1312-1318`) forwards only
   `all_pane_contents` into `Event::PaneRenderReport`. The ANSI map is
   computed on every report and then dropped before it ever reaches a
   plugin.
3. The one consumer that does receive styled contents is the client-side
   subscription path, not the plugin path:
   `SubscribeToPaneRenders { ansi: true }` is a `ClientToServerMsg`
   (`zellij-utils/src/ipc.rs:212`, routed at `zellij-server/src/route.rs:2743`),
   delivered via `ServerToClientMsg::PaneRenderUpdate`. There is no
   `PluginCommand` equivalent of this subscription, so a WASM plugin has no
   way to opt into it.
4. `set_pane_regex_highlights` (`zellij-tile/src/shim.rs:2962`) does let a
   plugin overlay styling on a real pane while preserving its colours, but
   highlights are expressed as `RegexHighlight`
   (`zellij-utils/src/data.rs:1318`) — regex-pattern matches, not row/column
   ranges. It cannot express an arbitrary selection span and cannot express
   block (rectangular) selection at all, so it is not a substitute for a
   selection-rendering primitive.

A patch enabling styled scrollback for plugins is being prepared in-tree at
`dev/patches/` as a sibling task. This is necessarily a **server-side**
change — it requires running a patched Zellij binary, not merely a patched
plugin — because nothing in `zellij-tile` can alter what the server chooses
to send over the wire; the gaps above are all on the server/protocol side.

## The zclip-side work

This issue is the plugin-side half: once styled scrollback text is
available (patched upstream) or unavailable (stock upstream), zclip must
handle both without corrupting its own logic.

The moment scrollback lines contain ANSI escape sequences, every column
calculation in `zclip-core` breaks, because all of it currently operates on
`char`s, and escape bytes are chars like any other. Concretely at risk:

- Cursor motion (`apply_motion`, `crates/zclip-core/src/motion.rs`) —
  word/line/column motions would count escape-sequence bytes as columns, so
  the cursor drifts away from what the user actually sees on screen.
- `selected_columns_for_row` and the selection model
  (`crates/zclip-core/src/selection.rs`, `region.rs`) — column ranges
  computed against styled text would not line up with visible columns.
- `extract_span` and yank — yanked text would contain raw escape sequences,
  poisoning the buffer ring and anything later pasted from it into a real
  shell.
- `render_copy_mode`'s width truncation in `crates/zclip/src/main.rs`, which
  currently does `line.chars().take(width)` — this would happily slice
  through the middle of an escape sequence and corrupt the terminal.
- `highlight()` in the same file, which indexes by char position and would
  be indexing into the wrong coordinate space once escape bytes are mixed
  in.

The required design is a separation of **logical text** (styling stripped —
the single source of truth for all motion, selection, yank and search
arithmetic) from **styled text** (retained purely for rendering). A row
needs both representations plus a mapping from logical column to styled
byte offset, so the renderer can re-apply the original styling while all
selection maths stays entirely in logical columns.

`highlight()` was already prepared for this, whether by design or luck: it
terminates a selection with `\x1b[27m` (reverse-video off) rather than
`\x1b[0m` (full reset), specifically so it toggles reverse video on top of
existing attributes the way tmux does, instead of destroying the line's own
colours. That convention should be kept and extended, not replaced.

## Acceptance criteria

- [ ] `zclip-core` defines a row representation that carries both logical
      (unstyled) text and styled text, plus a mapping from logical column to
      styled byte offset.
- [ ] `apply_motion` and all cursor motions operate purely on logical
      columns; unit tests feed styled input and assert identical cursor
      results to the plain-text equivalent.
- [ ] `selected_columns_for_row` and the selection model operate purely on
      logical columns, with the same styled-vs-plain equivalence tests as
      motion.
- [ ] `extract_span`/yank strips styling and always produces clean,
      escape-free text from styled input, verified by a unit test.
- [ ] Width truncation for rendering is escape-sequence aware: it never cuts
      a line in the middle of an escape sequence, and a truncated styled
      line still parses as valid ANSI.
- [ ] Selection rendering preserves the underlying pane colours and toggles
      only reverse video over the selected span (continuing the existing
      `\x1b[27m` convention), rather than resetting all attributes.
- [ ] When running against an unpatched Zellij (no styled-scrollback data
      available), zclip degrades gracefully to today's monochrome
      rendering — this feature must not hard-depend on the upstream patch.
- [ ] All of the above are covered by unit tests in `zclip-core` that run
      natively, without any `zellij-tile` host API.

## Technical notes

- `PluginCommand::GetPaneScrollback` has no ansi flag
  (`zellij-utils/src/data.rs:3534`); the server-side handler strips colour
  (`zellij-server/src/screen.rs:9196`).
- `pane_contents_with_ansi` (`zellij-server/src/panes/terminal_pane.rs:1290`)
  is computed and available server-side, and lands in
  `PaneRenderReport.all_pane_contents_with_ansi`
  (`zellij-utils/src/data.rs:2454`), but is dropped before reaching plugins
  in `WasmBridge::handle_pane_render_report`
  (`zellij-server/src/plugins/wasm_bridge.rs:1312-1318`).
- The client-only styled path (`SubscribeToPaneRenders { ansi: true }`,
  `zellij-utils/src/ipc.rs:212`, `zellij-server/src/route.rs:2743`,
  `ServerToClientMsg::PaneRenderUpdate`) has no `PluginCommand` equivalent.
- `set_pane_regex_highlights` (`zellij-tile/src/shim.rs:2962`,
  `RegexHighlight` at `zellij-utils/src/data.rs:1318`) preserves colour but
  is regex-based and cannot express selection ranges or block selection.
- `zclip-core` must remain hermetic — no `zellij-tile` dependency — so these
  tests run natively on the host target, matching the existing pattern in
  issue 16.
- Only the ANSI subset relevant to colour and text attributes (SGR
  sequences) needs to be stripped/parsed here. This is not a general
  terminal emulator; cursor-positioning and other control sequences are out
  of scope for logical-text extraction.

## Out of scope

- Actually landing the enabling change in upstream Zellij; that is tracked
  by the sibling `dev/patches/` task, not this issue.
- Clipboard/OSC 52 styling — the system clipboard bridge (M3) always
  receives plain text regardless of what copy mode renders.
- Any full terminal emulator in `zclip-core`; only SGR colour/attribute
  parsing is in scope.

## Depends on

Issue 21 (read pane scrollback into a selectable buffer) for the underlying
buffer model this extends. Also depends on the upstream styled-scrollback
capability described above as an external, not-yet-landed dependency; the
graceful-degradation acceptance criterion exists specifically so this issue
can ship ahead of that landing.
