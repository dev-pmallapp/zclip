# Zellij patches

## `zellij-0.45.1-plugin-ansi-scrollback.patch`

Lets a WASM plugin ask for **ANSI-styled** pane scrollback (SGR escape
sequences for colors/attributes) instead of plain text, by threading a
`with_ansi: bool` flag through the existing `GetPaneScrollback` plugin
command.

### Why this is needed

Zellij already renders styled scrollback server-side — it's just not
reachable from a plugin:

- `pane_contents_with_ansi()` exists and works
  (`zellij-server/src/panes/terminal_pane.rs:1290`, delegating to
  `Grid::pane_contents_with_ansi` in `panes/grid.rs`). It's what backs the
  `SubscribeToPaneRenders { ansi: true }` client IPC path.
- But `SubscribeToPaneRenders` is a **client-only** message
  (`zellij-utils/src/ipc.rs:212`) — it's not part of the plugin API, so a
  WASM plugin can't send it.
- The plugin-facing render report, `PaneRenderReport`, actually computes
  *both* a plain map and a styled map
  (`all_pane_contents` / `all_pane_contents_with_ansi` in
  `zellij-utils/src/data.rs`), but `handle_pane_render_report` in
  `zellij-server/src/plugins/wasm_bridge.rs:1312-1318` only forwards
  `all_pane_contents` to plugins. The ANSI map is computed and then
  silently dropped before it ever reaches WASM.
- The one plugin command that *does* let you pull scrollback on demand,
  `PluginCommand::GetPaneScrollback`, has no ANSI flag at all. Its handler
  in `zellij-server/src/screen.rs` (around line 9196) unconditionally calls
  `pane.pane_contents(...)`, never `pane.pane_contents_with_ansi(...)`.

So a plugin that wants to recolor, re-render, or otherwise preserve the
original styling of scrollback (which is exactly what `zclip`'s copy-mode
and buffer preview want to do) has no path to it today. This patch opens
one, by reusing machinery that already exists and is already exercised by
the client code path — no new rendering logic, no new permission surface.

### What it changes

Threads `with_ansi: bool` through the full round trip:

1. `zellij-utils/src/plugin_api/plugin_command.proto` — `GetPaneScrollbackPayload`
   gains `bool with_ansi = 3;`.
2. `zellij-utils/assets/prost/api.plugin_command.rs` — the checked-in
   generated prost struct for that message gains the matching
   `#[prost(bool, tag="3")] pub with_ansi: bool` field. (Zellij ships this
   file pre-generated rather than running `prost-build` at compile time —
   see the comment in `zellij-utils/src/plugin_api/mod.rs` — so it has to be
   hand-edited in lockstep with the `.proto` file.)
3. `zellij-utils/src/plugin_api/plugin_command.rs` — both protobuf
   conversion directions (`TryFrom` in each direction) pass the new field
   through.
4. `zellij-utils/src/data.rs` — `PluginCommand::GetPaneScrollback` gains a
   `with_ansi: bool` field.
5. `zellij-tile/src/shim.rs` — `get_pane_scrollback()`'s signature is
   **unchanged** (it still requests plain text), so no existing plugin
   breaks. A new sibling, `get_pane_scrollback_with_ansi(pane_id,
   get_full_scrollback)`, requests the styled variant. Its doc comment
   flags that returned lines carry SGR escapes and that callers doing
   column arithmetic must account for that.
6. `zellij-server/src/plugins/zellij_exports.rs` — the dispatch arm and
   `get_pane_scrollback()` thread the flag to the screen thread. The
   permission gate (`PluginCommand::GetPaneScrollback { .. } =>
   PermissionType::ReadPaneContents`) is untouched: styled scrollback is the
   same underlying pane data a plugin can already read, so no new
   permission is warranted.
7. `zellij-server/src/screen.rs` — `ScreenInstruction::GetPaneScrollback`
   gains the field; its handler calls `pane.pane_contents_with_ansi(...)`
   when set, else the existing `pane.pane_contents(...)`. The
   `ScreenContext` mapping uses a `{ .. }` pattern and needs no change.

### Wire compatibility

Adding a new protobuf field number is backwards compatible. An old plugin
(or old client) that never sets `with_ansi` serializes without it, and a
patched server decodes the missing field as `false` — today's plain-text
behaviour is preserved exactly. Nothing about the existing
`get_pane_scrollback()` call path changes.

### This is a server-side patch

`pane_contents_with_ansi` runs on the Zellij server, not inside the plugin.
Using this feature means running a **patched `zellij` binary**; a
patched/newer `zclip.wasm` alone is not enough, and a stock 0.45.1 (or any
unpatched) server will simply never see `with_ansi: true` take effect,
because it doesn't know the field exists. Wire compatibility means it
won't error either way — it just silently returns plain text, as before.

`zclip` must therefore treat the ANSI path as an optional enhancement and
**degrade gracefully to monochrome** when running against an unpatched
Zellij: call `get_pane_scrollback_with_ansi`, and if the host is an
unpatched server, the styling will simply be absent from the response
(never an error) — render it as plain text in that case rather than
assuming color data is present.

### Applying it

```sh
git clone https://github.com/zellij-org/zellij.git
cd zellij
git checkout v0.45.1
git apply -p1 /path/to/zellij-0.45.1-plugin-ansi-scrollback.patch
cargo build --release
```

### Verification performed

This patch was **not compiled** — a full Zellij build pulls in system
libraries (openssl, curl) not readily available in this environment, and
is large enough to make that impractical here. What was actually checked:

- `git apply --check -p1` against a pristine 0.45.1 tree (constructed from
  the same crates.io sources this patch was authored against): applies
  cleanly, no fuzz, no rejects.
- Every edited hunk was re-read by hand for syntactic correctness and for
  exhaustiveness (no partially-updated struct literals or match arms).
- `rustfmt --check` was run over each touched file as a parse sanity check
  (rustfmt requires valid syntax to run at all); it reported only
  pre-existing formatting-style diffs from this codebase's brace style, no
  parse errors.
- `grep -rn GetPaneScrollback` was run across all of `zellij-utils`,
  `zellij-tile`, and `zellij-server` to confirm every construction and
  destructuring site of both `PluginCommand::GetPaneScrollback` and
  `ScreenInstruction::GetPaneScrollback` was found and updated (including
  `ScreenContext`'s `{ .. }` mapping arm and the permission gate's `{ .. }`
  arm, which are wildcard patterns and correctly need no change).

**Not verified:** that the patched crate actually compiles end-to-end, or
that `pane_contents_with_ansi` produces the expected output at runtime
against a live session. Treat this patch as reviewed-but-unbuilt until
someone runs it through `cargo build --release` against a real checkout.
