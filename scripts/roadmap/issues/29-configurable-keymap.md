---
title: Make the copy-mode key table configurable
milestone: M2 - Copy Mode & Selection
labels: story,area:core,area:plugin,priority:p1
---

## Context

Every copy-mode key is hardcoded today. `Zclip::motion_for`
(`crates/zclip/src/main.rs:264`) is a fixed `match` over `BareKey` with vi
bindings baked in (`h`/`j`/`k`/`l`, `w`/`b`/`e`, `Ctrl u`/`Ctrl d`, and so on).
`handle_copy_mode_key` (`crates/zclip/src/main.rs:340`) hardcodes `y` for yank
(`:395`), `v`/`Ctrl v`/`V` for the three selection shapes (`:372-377`), and
`Esc` for cancel/clear-selection (`:350`). None of it reads `self.config`.

Issue #23 already called for exactly this: "a configurable key table…
data-driven (e.g. a map from `KeyWithModifier` to a motion enum) so new
bindings can be added without touching dispatch logic," plus a vi-style and
an emacs-style preset (`23-motion-keytable.md:13`). That criterion is
unticked. This issue delivers it, and widens the scope from motions alone to
the whole copy-mode surface — motions, selection shapes, yank, and cancel —
because all four live behind the same hardcoded `match` and splitting them
would just move the problem around.

This issue **supersedes the key-table bullets in #22, #23 and #24**:
- #23's "configurable key table… vi-style and emacs-style preset" bullet
  (`23-motion-keytable.md:13`) and its "data-driven… map from `KeyWithModifier`
  to a motion enum" bullet (`:15`).
- #22's acceptance criterion that a configured keybinding enters copy mode
  (`22-copy-mode-keys.md:11`) is unaffected — that is the Zellij-side
  `MessagePlugin`/`LaunchOrFocusPlugin` binding settled by #27, not a copy-mode
  internal key. Only the *internal* key surface #22 alludes to is in scope
  here.
- #24 does not itself list a key-table criterion, but its selection-shape
  keys (`v`/`V`/`Ctrl v`) are exactly what `handle_copy_mode_key` hardcodes,
  so #24's acceptance criteria are read as satisfied through this table, not
  through separate hardcoded arms.

## Why this cannot be Zellij keybinds

This is the first thing any reader will propose, so it needs to be closed off
explicitly. Two independent reasons, both already established by #27's
research into `zellij-client-0.45.1`/`zellij-server-0.45.1`:

1. **Interception only ever delivers keys UNBOUND in the client's current
   input mode.** The client forwards every key to the server raw, unresolved
   (`zellij-client/src/input_handler.rs:304`). The server resolves the key
   against the client's keybinds for its input mode first
   (`zellij-server/src/route.rs:2358`); a bound key becomes its action and
   never reaches `Action::Write`. Only the unbound default action
   (`Action::Write`, `zellij-server/src/route.rs:274`) is checked against the
   `keybind_intercepts` map (`zellij-server/src/screen.rs:8541`). Binding `y`
   to something in Zellij's own `keybinds` block therefore guarantees copy
   mode **never sees `y`** — the exact opposite of what a key table needs.
2. **Zellij's `InputMode` is a fixed, closed enum.** A plugin cannot define a
   "copy mode" `InputMode` to scope bindings to. So
   `bind "y" { MessagePlugin "zclip" { name "yank"; }; }` placed under
   `normal` would hijack the letter `y` for the *entire* session, in every
   pane, all the time — the user could never type a `y` into a shell again.
   There is no Zellij-side mode to confine the binding to only-while-copy-mode-
   is-open.

A third option was considered and rejected: switching the user into an
existing Zellij `InputMode` via `switch_to_input_mode` while copy mode is
open (floated as a possibility in #22, `22-copy-mode-keys.md:11`). That still
requires the mode's keybinds to be user-configured Zellij-side to cover the
copy-mode vocabulary, and it would hijack whatever real mode is chosen (e.g.
`locked` or a custom mode) and fight whatever bindings the user already has
there. It solves neither problem above.

Conclusion: the key table has to live in zclip's own plugin configuration
block, which `load()` already receives as a flat `BTreeMap<String, String>`
(`crates/zclip/src/main.rs:640`). The consolation is that the *syntax* does
not need to be invented: `KeyWithModifier` implements `FromStr`
(`zellij-utils-0.45.1/src/data.rs:92`), which delegates to `BareKey::from_str`
(`:243`) for the key name and parses leading modifier words itself — the same
grammar Zellij's own `bind "Ctrl v" { ... }` uses. `"Ctrl v"`, `"PageUp"`,
`"Alt y"` all parse with this one call. Same syntax as Zellij keybinds,
different location — the plugin config block, not the `keybinds` block.

## Design

Configuration lives in the `plugins { zclip { ... } }` block, alongside the
existing `buffer_limit` key:

- `keymap "vi"` selects a preset. `vi` is the default; `emacs` is also
  required, per #23.
- `key_<action> "..."` entries override individual actions on top of whichever
  preset is selected.
- Action name vocabulary (one per `CopyModeAction` variant): `left`, `down`,
  `up`, `right`, `word_forward`, `word_backward`, `word_end`, `line_start`,
  `line_first_non_blank`, `line_end`, `top`, `bottom`, `half_page_up`,
  `half_page_down`, `page_up`, `page_down`, `select_char`, `select_line`,
  `select_block`, `yank`, `cancel`.
- A value may bind more than one key to the same action. Resolution rule: try
  to parse the whole trimmed value as a single `KeyWithModifier` first, and
  only split on commas if that fails. This keeps the literal comma key
  bindable (`key_word_forward ","` binds the character `,`, it does not fail
  to parse and fall through to a split) while still allowing
  `key_yank "y, Enter"` to bind two keys to one action.

Architecture, respecting that `zclip-core` stays hermetic (no `zellij-tile`
dependency, so its tests run natively without a Zellij host — the pattern
already established by `parse_buffer_limit` in `crates/zclip-core/src/buffer.rs:209`
and by `SelectionMode`/motion logic in `region.rs`):

- **`zclip-core`** owns a `CopyModeAction` enum (wrapping the existing
  `Motion` and `SelectionMode` types, plus `Yank` and `Cancel` variants that
  neither currently models), the action-name string vocabulary above, the
  vi/emacs preset tables expressed as key *strings* (not `KeyWithModifier` —
  that type is host-side), and the merge/override logic that takes a preset
  plus a `BTreeMap<String, String>` of overrides and produces
  `(key_string, CopyModeAction)` pairs plus a list of human-readable warning
  strings for anything it could not apply. All of this is pure data and
  string manipulation — no Zellij types — so it is unit-testable the same way
  `parse_buffer_limit` is.
- **`zclip`** owns only the host-type conversion at the boundary: calling
  `KeyWithModifier::from_str` on the strings `zclip-core` produced, building
  the runtime lookup table from the results, and dispatching intercepted keys
  through it. `motion_for` (`main.rs:264`) and the hardcoded arms in
  `handle_copy_mode_key` (`main.rs:372-377`, `:395`) collapse into a single
  table lookup call.

Two safety properties are requirements, not nice-to-haves, and should be
called out as such in review:

- **Copy mode must always remain escapable.** If a user's config leaves
  `cancel` with no working binding (unset, unparseable, or overridden to a key
  that also fails), `Esc` is reinstated as the cancel binding regardless of
  what the config says. Copy mode holds the keyboard via
  `intercept_key_presses` (per #22/#27); an unescapable copy mode is a
  **trapped keyboard**, not a cosmetic bug, and must be treated with the
  severity that implies.
- **Malformed config must never prevent startup.** An unknown action name in
  a `key_*` entry, or a value that fails to parse as either a single key or a
  comma-separated list of keys, produces a warning and is skipped, falling
  back to whatever the active preset already bound for that action. This
  mirrors how `buffer_limit` already degrades today: an unparseable value logs
  a status message and falls back to `DEFAULT_BUFFER_LIMIT`
  (`crates/zclip/src/main.rs:641-651`) rather than refusing to load.

## Acceptance criteria

- [ ] `zclip-core` defines a `CopyModeAction` enum covering every `Motion`
      variant, every `SelectionMode` variant, `Yank`, and `Cancel`, plus the
      action-name string vocabulary listed above (one name per variant).
- [ ] `zclip-core` defines vi and emacs preset tables as `(action name, key
      string)` data, both selectable via `keymap "vi"` / `keymap "emacs"`.
- [ ] `zclip-core` exposes a merge function that layers `key_<action>`
      overrides from a `BTreeMap<String, String>` on top of a selected preset,
      returning the resulting bindings plus a list of warnings for anything
      rejected.
- [ ] The merge function implements the try-whole-value-first-then-split rule
      for multi-key bindings, including the literal comma case
      (`key_<action> ","` binds comma; `key_<action> "a, b"` binds two keys).
- [ ] `crates/zclip`'s dispatch is reduced to a lookup against the table
      built from `zclip-core`'s output; `motion_for`'s hardcoded `match`
      (`main.rs:264`) and the hardcoded selection-shape/yank arms in
      `handle_copy_mode_key` (`main.rs:372-377`, `:395`) are removed, not kept
      alongside the table.
- [ ] An unknown action name in a `key_*` config entry produces a warning
      (surfaced the same way `buffer_limit` errors are, via `self.status`) and
      is skipped without preventing plugin load.
- [ ] A `key_*` value that fails to parse (as a whole key, and after
      splitting on commas) produces a warning and is skipped, falling back to
      the preset's binding for that action.
- [ ] `Esc` is guaranteed to cancel copy mode regardless of configuration —
      verified by a test that overrides `key_cancel` to an unparseable or
      empty value and confirms `Esc` still works.
- [ ] Native unit tests in `zclip-core` (no Zellij host, no `zellij-tile`
      dependency) cover: preset construction for both vi and emacs, override
      merging (including an override replacing a preset binding), multi-key
      value parsing (including the comma-as-key case), and warning generation
      for unknown action names and unparseable values.
- [ ] The example KDL (#55) documents the full `key_*` table for at least the
      vi preset, including `keymap`.
- [ ] Documentation (example KDL or README) states the constraint from
      "Why this cannot be Zellij keybinds" above: keys bound in Zellij's own
      `keybinds` block never reach copy mode's interception, because
      interception only sees keys unbound in the current input mode.

## Technical notes

- Client forwards all keys unresolved: `zellij-client/src/input_handler.rs:304`.
- Server resolves keybinds before the intercept check:
  `zellij-server/src/route.rs:2358` (bound key -> action),
  `zellij-server/src/route.rs:274` (unbound key -> `Action::Write`),
  `zellij-server/src/screen.rs:8541` (`keybind_intercepts` consulted only in
  the `WriteCharacter` arm).
- `KeyWithModifier: FromStr` at `zellij-utils-0.45.1/src/data.rs:92`, using
  `BareKey::from_str` (`:243`) for the key name — this is the same parser
  Zellij's own KDL `bind` keys use, so key strings in the plugin config are
  written identically to Zellij keybind syntax.
- `Ctrl b` is bound to `SwitchToMode "Tmux"` in Zellij's default config
  (already recorded in `27-keybind-contract.md:91-95` and reflected in
  `main.rs:272-277`'s comment), so it never reaches the intercept and the vi
  preset's `Ctrl b` page-up entry is shadowed under default keybinds. `PageUp`
  is the working alternative and should be the documented default, not
  `Ctrl b`, unless the user has unbound it.
- `Event::PluginConfigurationChanged(BTreeMap<String, String>)` exists
  (`zellij-utils-0.45.1/src/data.rs:1027`) if live reconfiguration is ever
  wanted; not required for this issue (see Out of scope).
- The existing degrade-and-warn pattern to follow is
  `crates/zclip/src/main.rs:641-651` (`parse_buffer_limit` failure ->
  `self.status` message -> `DEFAULT_BUFFER_LIMIT` fallback).

## Out of scope

- Live config reload in response to `Event::PluginConfigurationChanged`.
- Rebinding the commands that live outside copy mode — `copy_mode` entry,
  `paste`, `list`, and top-level `cancel` — which are genuine Zellij
  keybinds reached via `MessagePlugin`/`LaunchOrFocusPlugin` and are already
  user-configurable through Zellij's own `keybinds` block (#27).
- Operator-pending or multi-key sequences, such as vi's `gg` (`main.rs:294-297`
  already notes this is deferred pending an operator/pending-input state
  machine).

## Depends on
Cursor motions and a vi/emacs key table; Char-wise, line-wise and block-wise selection
