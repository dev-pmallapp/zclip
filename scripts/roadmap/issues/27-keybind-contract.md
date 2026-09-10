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

## Acceptance criteria

- [ ] `pipe()` dispatches on `PipeMessage::name`, handling at least `paste` and `list`
- [ ] `paste` writes the most recent buffer into the **currently focused terminal pane**, resolved via `get_focused_pane_info()`, and works while zclip is unfocused or hidden
- [ ] `paste` accepts an optional buffer selector in `PipeMessage::payload` (buffer name, or index) and falls back to most-recent
- [ ] Pressing the copy-mode keybinding opens zclip floating and focused, ready for keyboard navigation
- [ ] The M1 focused-only `y`/`p` key handlers are **removed**, not kept alongside; two competing input paths would be a maintenance trap
- [ ] Confirm whether `PipeSource::Keybind` requires the `ReadCliPipes` permission (the source only documents it for *CLI* pipes) and request it only if actually needed
- [ ] Confirm a `launch_new` background plugin may call `write_chars_to_pane_id` into the focused terminal
- [ ] The example KDL (#39) ships this exact keybinding block
- [ ] Manual test: yank something, focus an unrelated pane, hit the paste binding, and the text appears in that pane

## Technical notes

- `MessagePlugin` KDL children: `name`, `payload`, `launch_new`, `skip_cache`, `floating`, `title`, `cwd`
- `get_focused_pane_info() -> Result<(usize, PaneId), String>` (`shim.rs:208`) gives the paste target; `get_focused_pane(tab, &manifest)` (`shim.rs:2819`) filters out plugin panes
- `hide_self()` / `show_self(should_float_if_hidden: bool)` (`shim.rs:879`/`895`) let a resident plugin stay invisible until summoned — see #38
- Writing is `write_chars_to_pane_id(chars: &str, pane_id: PaneId)` (`shim.rs:1930`), gated on `WriteToStdin`
- Keys already reach the plugin pane via `Event::Key` with no extra permission — verified live in a scratch session

## Out of scope

- Cursor motion and selection logic inside copy mode (#17, #18)
- Scrollback rendering (#16)
- CLI-driven `zellij pipe` (#36) — this issue builds the `pipe()` dispatch that #36 extends

## Depends on
Paste: write buffer contents into a target pane
