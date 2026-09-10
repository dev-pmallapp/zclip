---
title: Bind zclip to keys: copy-mode entry and paste into the current pane
milestone: M2 - Copy Mode & Selection
labels: story,area:config,area:core,priority:p0
---

## Context

M1 shipped a yank/paste engine that is **not usable as a tmux replacement**, and
this issue exists to correct that. Three concrete problems with the M1 slice:

1. **Yank requires a mouse.** `Zclip::yank` reads `PaneContents::selected_text`,
   which the host only populates after a mouse drag. There is no keyboard path
   to select text at all, so the core tmux workflow is impossible.
2. **Paste only works while the zclip pane is focused.** That is backwards:
   tmux pastes into the pane you are working in. Requiring the user to focus the
   clipboard plugin before pasting defeats the purpose.
3. **There are no keybindings.** The plugin must be launched by hand.

The root cause is that #10 was specified as "read the selection" and implemented
literally, without asking whether the result was a usable workflow. It is not.

## The contract this issue delivers

Reproduce tmux's `prefix [` / `prefix ]` flow:

```kdl
keybinds {
    normal {
        // prefix [ -- enter copy mode over the pane you are leaving
        bind "Ctrl [" { LaunchOrFocusPlugin "zclip" { floating true; }; }

        // prefix ] -- paste the most recent buffer into the CURRENT pane
        bind "Ctrl ]" { MessagePlugin "zclip" { name "paste"; launch_new true; }; }

        // prefix = -- buffer browser
        bind "Ctrl =" { MessagePlugin "zclip" { name "list"; floating true; }; }
    }
}
```

`MessagePlugin` is the key mechanism and it is **verified to exist**:
`zellij-utils-0.45.1/src/kdl/mod.rs:2244` parses it into
`Action::KeybindPipe { name, payload, plugin, launch_new, skip_cache, floating, .. }`,
which reaches the plugin as `pipe()` with `PipeSource::Keybind`. Because it can
`launch_new`, a keybinding can reach a *background* zclip while a terminal pane
keeps focus — which is exactly what paste-from-any-pane requires.

## The keybind contract, settled

Verified by reading `zellij-client-0.45.1` and `zellij-server-0.45.1` from
crates.io: **interception captures unbound keys only.** `intercept_key_presses`
cannot pre-empt a keybinding; it only ever sees a key that would otherwise have
been typed into the pane.

The evidence chain:

1. `zellij-client/src/input_handler.rs:304` — the client resolves nothing.
   Every key goes to the server as `ClientToServerMsg::Key`, explicitly so
   keybinds can change at runtime.
2. `zellij-server/src/route.rs:2358` — the server resolves the key against the
   client's keybinds for its input mode via
   `get_actions_for_key_in_mode_or_default_action`. A bound key becomes its
   action (`Alt n` -> `Action::NewPane`); only an unbound key falls through to
   the default action `Action::Write { .. }`.
3. `zellij-server/src/route.rs:274` — `Action::Write` is the sole producer of
   `ScreenInstruction::WriteCharacter`.
4. `zellij-server/src/screen.rs:8541` — the `keybind_intercepts` map
   (populated by `InterceptKeyPresses` at `screen.rs:12166`) is consulted ONLY
   inside the `WriteCharacter` arm.

Corollaries:

- The user's own keybindings keep firing while copy mode is open. `Alt n`
  still opens a new pane; a user's `Alt q` -> `cancel` binding genuinely
  works. This was previously assumed to be impossible.
- The `screen.rs:8541` lookup is keyed on `client_id` alone and never
  consults focus, so unbound keys are captured even while a **terminal** pane
  is focused. Copy mode does not need to hold focus to receive keys.
- The mechanism that DOES force every key down the `Write` path is
  `key_passthrough_clients` (`route.rs:2343-2357`). It is driven solely by
  the nested-guest (nested Zellij session) machinery in `screen.rs` and is
  NOT reachable from any `PluginCommand`. There is no plugin-accessible way
  to pre-empt keybinds.
- Under zellij's default config, the keys bound in Normal mode -- and
  therefore invisible to copy mode -- are `Ctrl g/q/p/n/s/o/t/h/b` and the
  `Alt` set `f n i o h l j k = + - [ ] p`, `Alt Shift p`, `Alt` arrows.
- `Esc` is bound only in `shared_except "normal" "locked"`, so it is UNBOUND
  in Normal mode and IS intercepted. Copy mode handling `Esc` itself is
  correct.
- One real collision with zclip's copy-mode key table: `Ctrl b` is bound to
  `SwitchToMode "Tmux"` by default, so copy mode's `Ctrl-b` PageUp never
  arrives. `Ctrl u`, `Ctrl d`, `Ctrl f`, `Ctrl v` are unbound by default and
  work. The working alternative is the `PageUp` key, already in the motion
  table.

## Acceptance criteria

- [ ] `pipe()` dispatches on `PipeMessage::name`, handling at least `paste` and `list`
- [ ] `paste` writes the most recent buffer into the **currently focused terminal pane**, resolved via `get_focused_pane_info()`, and works while zclip is unfocused or hidden
- [ ] `paste` accepts an optional buffer selector in `PipeMessage::payload` (buffer name, or index) and falls back to most-recent
- [ ] Pressing the copy-mode keybinding opens zclip floating and focused, ready for keyboard navigation
- [ ] The M1 focused-only `y`/`p` key handlers are **removed**, not kept alongside; two competing input paths would be a maintenance trap
- [ ] Confirm whether `PipeSource::Keybind` requires the `ReadCliPipes` permission (the source only documents it for *CLI* pipes) and request it only if actually needed
- [ ] Confirm a `launch_new` background plugin may call `write_chars_to_pane_id` into the focused terminal
- [ ] The example KDL (#39) ships this exact keybinding block
- [ ] The example KDL (#39) documents the default `Ctrl b` -> `SwitchToMode "Tmux"` collision with copy mode's PageUp binding, and shows the `PageUp` key as the working alternative
- [ ] The shipped docs state plainly that the user's own keybindings remain live while copy mode is open -- interception only ever catches keys unbound in the current input mode
- [ ] Manual test: yank something, focus an unrelated pane, hit the paste binding, and the text appears in that pane

## Technical notes

- `MessagePlugin` KDL children: `name`, `payload`, `launch_new`, `skip_cache`, `floating`, `title`, `cwd`
- `get_focused_pane_info() -> Result<(usize, PaneId), String>` (`shim.rs:208`) gives the paste target; `get_focused_pane(tab, &manifest)` (`shim.rs:2819`) filters out plugin panes
- `hide_self()` / `show_self(should_float_if_hidden: bool)` (`shim.rs:879`/`895`) let a resident plugin stay invisible until summoned — see #38
- Writing is `write_chars_to_pane_id(chars: &str, pane_id: PaneId)` (`shim.rs:1930`), gated on `WriteToStdin`
- Keys already reach the plugin pane via `Event::Key` with no extra permission — verified live in a scratch session
- Interception captures unbound keys only, verified in `zellij-client-0.45.1`/`zellij-server-0.45.1`: `input_handler.rs:304` (client forwards raw), `route.rs:2358` (server resolves keybind vs. default `Action::Write`), `route.rs:274` (`Action::Write` -> `WriteCharacter`), `screen.rs:8541` (`keybind_intercepts` consulted only in the `WriteCharacter` arm, populated by `InterceptKeyPresses` at `screen.rs:12166`)
- `key_passthrough_clients` (`route.rs:2343-2357`) is the only thing that forces all keys through `Write`, and it is nested-guest-only, not reachable from any `PluginCommand`

## Out of scope

- Cursor motion and selection logic inside copy mode (#17, #18)
- Scrollback rendering (#16)
- CLI-driven `zellij pipe` (#36) — this issue builds the `pipe()` dispatch that #36 extends

## Depends on
Paste: write buffer contents into a target pane
