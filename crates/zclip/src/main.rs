//! zclip — a Zellij plugin providing a tmux-like yank-buffer ring.
//!
//! This crate is a thin shim over the Zellij host API: it wires [`Zclip`] into
//! the plugin lifecycle (`load` / `update` / `pipe` / `render`) via
//! [`register_plugin!`] and translates host events into calls on `zclip-core`.
//!
//! Keep behaviour out of here. Anything that can be expressed without the host
//! API belongs in `zclip-core`, where it can be unit-tested natively — this
//! crate can only be meaningfully built for `wasm32-wasip1`, because linking it
//! natively drags in the whole of `zellij-utils` (curl, openssl, tokio, …).

use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;

use zclip_core::{
    action_name, apply_motion, parse_buffer_limit, resolve_keymap, split_key_spec, BufferRing,
    CopyMode, CopyModeAction, Cursor, PermissionState, Scrollback, SelectionMode,
    DEFAULT_BUFFER_LIMIT,
};
use zellij_tile::prelude::*;

/// Permissions zclip requests at load time.
///
/// - `ReadApplicationState` — receive `PaneUpdate`, so we know which terminal
///   pane the user was last in (the yank source and paste target).
/// - `ReadPaneContents` — `get_pane_scrollback`, to read the selection.
/// - `WriteToStdin` — `write_chars_to_pane_id`, to paste.
/// - `InterceptInput` — grab whatever keys are *unbound* in the user's current
///   input mode during copy mode, so vi motions work without the user having
///   to bind each one by hand. Keys the user HAS bound are resolved by Zellij
///   before the intercept is ever consulted, so those keypresses still act on
///   the binding and never arrive here. Only held while copy mode is active;
///   see `enter_copy_mode` / `exit_copy_mode`.
/// - `ChangeApplicationState` — `replace_pane_with_existing_pane`, to swap
///   zclip's own pane in for the pane being copied from on entering copy mode,
///   and swap it back out again on exit. See `enter_copy_mode` /
///   `exit_copy_mode`.
///
/// - `WriteToClipboard` — `copy_to_clipboard`, backing the `yank_clipboard`
///   copy-mode action. Note this is *not* the same capability as shelling out
///   to `xclip`/`wl-copy`: `copy_to_clipboard` hands the text to Zellij, which
///   writes it wherever the user's own `copy_command`/`copy_clipboard` config
///   already points. That is why zclip can reach the system clipboard without
///   `RunCommands`, and why it inherits a working clipboard setup instead of
///   second-guessing one.
///
/// Capabilities for later milestones are deliberately *not* requested, so users
/// are not asked to approve something zclip cannot yet do: `RunCommands`
/// (shell-out clipboard backends and OSC 52 fallback, M3), `ReadCliPipes`
/// (`zellij pipe`, M5).
const REQUIRED_PERMISSIONS: &[PermissionType] = &[
    PermissionType::ReadApplicationState,
    PermissionType::ReadPaneContents,
    PermissionType::WriteToStdin,
    PermissionType::InterceptInput,
    PermissionType::ChangeApplicationState,
    PermissionType::WriteToClipboard,
];

/// How many characters of a buffer to show per row in the list.
const PREVIEW_WIDTH: usize = 60;

/// Window height assumed when entering copy mode.
///
/// `pipe()` has no idea how tall the pane is — only `render` is told — so entry
/// picks a plausible value and the first render immediately reconciles the
/// viewport against the real height. Nothing user-visible depends on this being
/// right.
const PROVISIONAL_HEIGHT: usize = 24;

/// Top-level plugin state.
#[derive(Default)]
pub struct Zclip {
    /// Raw key/value configuration parsed from the plugin's KDL block.
    config: BTreeMap<String, String>,
    /// Gate for every permission-requiring host call. See [`Zclip::ready`].
    permissions: PermissionState,
    /// The paste-buffer ring itself. All the interesting logic lives here.
    ring: BufferRing,
    /// Our own pane id, used to swap ourselves into and out of the target
    /// pane's slot via `replace_pane_with_existing_pane`. See
    /// `enter_copy_mode` / `exit_copy_mode`.
    own_plugin_id: Option<u32>,
    /// The most recent *terminal* pane observed focused: both the yank source
    /// and the paste target. `None` until the first `PaneUpdate` arrives.
    target_pane: Option<PaneId>,
    /// The pane zclip displaced to enter copy mode, if any.
    ///
    /// Recorded at swap-in time and consumed at swap-out time so exit
    /// restores exactly the pane that was replaced -- `target_pane` is not
    /// safe to use for this, since `PaneUpdate` keeps it pointed at whatever
    /// terminal is currently focused, which may have moved on while copy mode
    /// was active.
    copy_mode_origin: Option<PaneId>,
    /// Result of the last action, shown in the UI so that a no-op (empty
    /// selection, no target pane) explains itself instead of looking broken.
    status: Option<String>,
    /// Copy-mode lifecycle. `Inactive` means we render the buffer list.
    copy_mode: CopyMode,
    /// Id of the most recent CLI pipe handled, so the trailing end-of-stream
    /// message of that same pipe does not re-run the command. See `pipe`.
    last_cli_pipe: Option<String>,
    /// Which event delivered the most recent key, and what it was.
    ///
    /// Diagnostic. `Event::Key` and `Event::InterceptedKeyPress` are handled
    /// identically, so without this there is no way to tell from the outside
    /// whether `InterceptInput` is actually doing anything -- the fallback path
    /// looks the same to a user. Shown in the copy-mode footer.
    last_key_source: Option<(&'static str, String)>,
    /// Text-window height from the most recent render.
    ///
    /// Paging motions need the window size, but key events carry no geometry --
    /// only `render` is told the pane dimensions. This is the last known good
    /// value rather than a guess; it is refreshed on every render, so it can
    /// only be stale for the single keypress following a resize.
    last_render_height: usize,
    /// The resolved copy-mode key table: which [`KeyWithModifier`] does which
    /// [`CopyModeAction`].
    ///
    /// Built ONCE, in `load`, from `zclip-core`'s [`resolve_keymap`] plus this
    /// crate's own `KeyWithModifier::from_str` parsing -- not derived from
    /// Zellij's own `keybinds` block. It cannot be: interception
    /// (`InterceptInput`) only ever delivers keys that are UNBOUND in the
    /// user's current input mode, so a key bound in `keybinds` never reaches
    /// here to be matched against; and `InputMode` is a fixed, closed enum
    /// with no "copy mode" variant a plugin could scope bindings to, so there
    /// is nowhere Zellij-side to hang a copy-mode-only table even if the first
    /// problem did not exist. See issue #29 for the full argument. A
    /// `HashMap` is safe here despite `Char('V')` and `Char('v')` needing to
    /// stay distinct keys, because `KeyWithModifier`'s `Hash` normalises an
    /// uppercase `Char` to lowercase-plus-`Shift` exactly the way its `Eq`
    /// does, so the two never collide.
    keys: HashMap<KeyWithModifier, CopyModeAction>,
}

impl Zclip {
    /// Whether gated host calls may be made.
    ///
    /// Calling a gated Zellij API before permissions are granted is a silent
    /// no-op rather than an error, so every gated call site must check this.
    fn ready(&self) -> bool {
        self.permissions.is_ready()
    }

    /// Records the most recently focused terminal pane.
    ///
    /// Panes are only *replaced*, never cleared: while the user is interacting
    /// with the zclip pane itself no terminal pane is focused, and forgetting
    /// the target at that exact moment would make yank and paste unusable.
    fn track_focused_pane(&mut self, manifest: &PaneManifest) {
        // `PaneInfo::is_focused` is per-*tab*: every tab reports its own
        // focused pane, so scanning the whole manifest finds several at once
        // and picking the first is a coin toss over `HashMap` iteration order.
        // Ask the host which tab the client is actually in before believing
        // any of them.
        let Ok((focused_tab, focused_pane)) = get_focused_pane_info() else {
            return;
        };

        // A terminal is focused: that is the answer, no search needed.
        if matches!(focused_pane, PaneId::Terminal(_)) {
            self.target_pane = Some(focused_pane);
            return;
        }

        // Otherwise the user is in a plugin (very likely us). Fall back to the
        // focused terminal *of that same tab*, so we remember where they were.
        if let Some(panes) = manifest.panes.get(&focused_tab) {
            if let Some(pane) = panes
                .iter()
                .find(|pane| pane.is_focused && !pane.is_plugin && !pane.exited)
            {
                self.target_pane = Some(PaneId::Terminal(pane.id));
            }
        }
    }

    /// Enters copy mode over the pane the user was last in.
    ///
    /// The scrollback is captured **once, on entry**, rather than tracked
    /// live. A pane that keeps producing output will therefore drift from what
    /// is displayed — that is a deliberate tradeoff (a moving target is
    /// unselectable), and the UI says so rather than pretending otherwise.
    fn enter_copy_mode(&mut self, height: usize) {
        // Re-entering would re-snapshot the pane and throw away an
        // in-progress selection. Copy mode is a single keypress away, so an
        // accidental double-press must not destroy work; treat the second
        // press as "show me the copy mode I already have".
        if self.copy_mode.is_active() {
            return;
        }

        let Some(pane_id) = self.target_pane else {
            self.status = Some("no terminal pane seen yet — focus one first".into());
            return;
        };

        // `true` = include scrollback history, not just the viewport. Unlike
        // the mouse-selection path, copy mode is precisely for reaching text
        // that has scrolled off screen.
        let contents = match get_pane_scrollback(pane_id, true) {
            Ok(contents) => contents,
            Err(err) => {
                self.status = Some(format!("could not read pane: {err}"));
                return;
            }
        };

        let scrollback = Scrollback::from_parts(
            contents.lines_above_viewport,
            contents.viewport,
            contents.lines_below_viewport,
        );

        if scrollback.is_empty() {
            self.status = Some("target pane has no content to copy".into());
            return;
        }

        let lines = scrollback.len();
        let pane_label = match pane_id {
            PaneId::Terminal(id) => format!("terminal_{id}"),
            PaneId::Plugin(id) => format!("plugin_{id}"),
        };
        // Capturing the scrollback above, before the swap, matters for two
        // reasons: it reads the target pane while it is still in place (post-
        // swap it would be suppressed), and a failed capture must not leave
        // zclip standing in for a pane it has nothing to show for.
        self.copy_mode.enter(scrollback, height);

        // Grab whatever keys are unbound in the user's current input mode, so
        // vi motions work without the user having to bind each one. This is
        // held ONLY while copy mode is active -- see `exit_copy_mode`, and the
        // `BeforeClose` handler which releases it if the pane is closed out
        // from under us. Leaking an intercept would leave the user's keyboard
        // captured by an invisible plugin.
        intercept_key_presses();

        // Swap zclip's own pane into the target's exact slot -- same tab,
        // same geometry -- rather than floating over it, so copy mode looks
        // like tmux's in-place `copy-mode` instead of a popup. `true` tells
        // the host to park the displaced pane in `suppressed_panes` rather
        // than closing it; `exit_copy_mode` reverses this with the mirror
        // call. If we somehow never learned our own plugin id, fall back to
        // the old floating behaviour so copy mode is still usable rather than
        // silently invisible.
        match self.own_plugin_id {
            Some(own_id) => {
                replace_pane_with_existing_pane(pane_id, PaneId::Plugin(own_id), true);
                self.copy_mode_origin = Some(pane_id);
            }
            None => show_self(true),
        }

        self.status = Some(format!("copy mode — {lines} line(s) from {pane_label}"));
    }

    /// Leaves copy mode and undoes the pane swap made on entry.
    ///
    /// Releasing the keyboard is unconditional rather than gated on the state
    /// transition: if the intercept were ever set without the session (or vice
    /// versa) the safe direction to fail is "release".
    ///
    /// Restoring the original pane is not cosmetic. `enter_copy_mode` swaps
    /// zclip's pane into the target's slot with `replace_pane_with_existing_pane`,
    /// and a plugin pane left standing there and focused is reported as the
    /// focused pane on the next `PaneUpdate` -- which makes `track_focused_pane`
    /// fall back to guessing a terminal, corrupting the paste target. The
    /// mirror call below puts the terminal back in its slot and re-suppresses
    /// zclip in one move, which is also what makes paste-from-any-pane work at
    /// all. If there is no recorded origin (`copy_mode_origin` was never set,
    /// e.g. `enter_copy_mode` fell back to `show_self` for lack of an own
    /// plugin id), fall back to plain `hide_self` -- this also covers the
    /// buffer list, which is revealed by `show_self` rather than by a swap.
    fn exit_copy_mode(&mut self) {
        clear_key_presses_intercepts();
        if self.copy_mode.exit() {
            self.status = Some("left copy mode".into());
            match (self.copy_mode_origin.take(), self.own_plugin_id) {
                (Some(origin), Some(own_id)) => {
                    replace_pane_with_existing_pane(PaneId::Plugin(own_id), origin, true);
                }
                _ => hide_self(),
            }
        }
    }

    /// Yanks the current selection into the ring and leaves copy mode,
    /// optionally also placing it on the system clipboard.
    ///
    /// Yanking with no active selection is a no-op with an explanation rather
    /// than silently exiting, which would look like the keypress was lost.
    ///
    /// `to_clipboard` distinguishes the two yank actions. Both write the ring;
    /// the clipboard is strictly additional, never instead of it (see
    /// [`CopyModeAction::YankToClipboard`]). The clipboard write is deliberately
    /// gated on the ring having accepted the text, so the two destinations can
    /// never disagree about what was yanked -- in particular a selection of
    /// nothing but padding, which the ring rejects as blank, must not silently
    /// clobber whatever the user already had on their clipboard.
    fn yank_selection(&mut self, to_clipboard: bool) {
        let Some(session) = self.copy_mode.session() else {
            return;
        };
        let Some(text) = session.selected_text() else {
            self.status = Some("nothing selected — press v, V or Ctrl-v first".into());
            return;
        };

        // Terminal grids pad rows out to the pane width, so a captured
        // selection carries meaningless trailing spaces on every line.
        let text = zclip_core::trim_trailing_blanks(&text);
        let lines = text.lines().count();

        // The ring rejects blank text, so selecting only padding yanks nothing.
        match self.ring.push(text.as_str()) {
            Some(_) => {
                if to_clipboard {
                    // Routes through Zellij's own configured clipboard
                    // destination rather than shelling out -- see
                    // REQUIRED_PERMISSIONS' note on WriteToClipboard. There is
                    // no result to check: the host takes the text and reports
                    // nothing back, so a misconfigured `copy_command` on the
                    // user's side fails silently here by construction.
                    copy_to_clipboard(&text);
                }
                self.exit_copy_mode();
                let destination = if to_clipboard { " + clipboard" } else { "" };
                self.status = Some(format!(
                    "yanked {lines} line(s) — {} in ring{destination}",
                    self.ring.len()
                ));
            }
            None => self.status = Some("selection was blank — nothing yanked".into()),
        }
    }

    /// Handles a key delivered to copy mode, whether intercepted or not.
    ///
    /// Interception only ever delivers keys that are UNBOUND in the user's
    /// current input mode -- Zellij resolves bound keys to their action
    /// before the intercept is consulted, so a user's `cancel` binding still
    /// fires and works during copy mode even though it never reaches this
    /// function. `Esc` typically DOES reach here, because it is unbound in
    /// Normal mode by default -- but that is no longer special-cased below;
    /// it is just whatever the table says `Esc` does, same as every other
    /// key. `load` guarantees some key maps to `Cancel` (reinstating `Esc`
    /// if nothing else does), so the escape hatch survives regardless.
    ///
    /// This table lookup also fixes a latent bug the old hardcoded
    /// `(BareKey::Char('V'), false)` arm had: some terminals report an
    /// uppercase letter as the *lowercase* `BareKey::Char` plus a `Shift`
    /// modifier rather than as the uppercase char itself, which that arm's
    /// exact-match would miss entirely -- falling through and being treated
    /// as plain `v` (char-wise selection) instead of `V` (line-wise).
    /// `KeyWithModifier`'s `PartialEq`/`Hash` normalise both spellings of
    /// `V` to the same value, so the table gets this right where the old
    /// `match` could not.
    fn handle_copy_mode_key(&mut self, key: &KeyWithModifier) -> bool {
        let Some(action) = self.keys.get(key).copied() else {
            return false;
        };

        match action {
            CopyModeAction::Cancel => {
                // Back out one level at a time, as in vi: drop an active
                // selection first and only leave copy mode once there is
                // nothing left to cancel. Exiting outright would discard a
                // painstakingly made selection on a single mis-keypress.
                let had_selection = self
                    .copy_mode
                    .session()
                    .is_some_and(zclip_core::CopySession::has_selection);
                if had_selection {
                    if let Some(session) = self.copy_mode.session_mut() {
                        session.clear_selection();
                    }
                    self.status = Some("selection cleared".into());
                } else {
                    self.exit_copy_mode();
                }
                true
            }
            CopyModeAction::Select(mode) => {
                let Some(session) = self.copy_mode.session_mut() else {
                    return false;
                };
                // Pressing the same shape twice cancels, as in vi.
                session.toggle_selection(mode);
                self.status = Some(if session.has_selection() {
                    match mode {
                        SelectionMode::Char => "char selection".into(),
                        SelectionMode::Line => "line selection".into(),
                        SelectionMode::Block => "block selection".into(),
                    }
                } else {
                    String::from("selection cleared")
                });
                true
            }
            CopyModeAction::Yank => {
                // Handles the empty-selection case itself, with an
                // explanatory status, rather than exiting -- see its own doc
                // comment.
                self.yank_selection(false);
                true
            }
            CopyModeAction::YankToClipboard => {
                self.yank_selection(true);
                true
            }
            CopyModeAction::Motion(motion) => {
                let Some(session) = self.copy_mode.session_mut() else {
                    return false;
                };
                let next = apply_motion(
                    motion,
                    session.scrollback(),
                    session.cursor(),
                    self.last_render_height,
                );
                session.set_cursor(next);
                true
            }
        }
    }

    /// Captures the current selection in the target pane into the ring.
    fn yank(&mut self) {
        let Some(pane_id) = self.target_pane else {
            self.status = Some("no terminal pane seen yet — focus one first".into());
            return;
        };

        // `false` = viewport only, no scrollback. This is deliberate: it keeps
        // `lines_above_viewport`/`lines_below_viewport` empty, which removes the
        // ambiguity about which line the selection's row 0 refers to. Reading
        // full scrollback is M2's problem, and will need that mapping settled.
        let contents = match get_pane_scrollback(pane_id, false) {
            Ok(contents) => contents,
            Err(err) => {
                self.status = Some(format!("could not read pane: {err}"));
                return;
            }
        };

        let Some(selection) = contents.selected_text else {
            self.status = Some("nothing selected in the target pane".into());
            return;
        };

        let lines: Vec<&str> = contents.viewport.iter().map(String::as_str).collect();
        let start = Cursor::new(
            selection.start.line.0.max(0) as usize,
            selection.start.column.0,
        );
        let end = Cursor::new(selection.end.line.0.max(0) as usize, selection.end.column.0);

        // Terminal grids pad every row out to the pane width, so a raw capture
        // is full of meaningless trailing spaces.
        let text = zclip_core::trim_trailing_blanks(&zclip_core::extract_span(&lines, start, end));

        // The ring rejects blank text itself, so an empty selection can never
        // create an entry.
        match self.ring.push(text) {
            Some(_) => {
                let count = self.ring.len();
                self.status = Some(format!("yanked ({count} in ring)"));
            }
            None => self.status = Some("selection was empty — nothing yanked".into()),
        }
    }

    /// Records the focused terminal pane, asking the host directly.
    ///
    /// Used on paths that are about to steal focus, where waiting for the next
    /// `PaneUpdate` would be too late.
    fn refresh_target_pane(&mut self) {
        if let Ok((_tab, pane_id)) = get_focused_pane_info() {
            if matches!(pane_id, PaneId::Terminal(_)) {
                self.target_pane = Some(pane_id);
            }
        }
    }

    /// The pane a paste should land in.
    ///
    /// Asks the host for the live focus first, because paste is normally
    /// triggered by a keybinding while the user is in a terminal pane and zclip
    /// is unfocused or hidden — in that situation the host's answer is the
    /// correct one. Falls back to the pane last seen focused via `PaneUpdate`,
    /// which covers the case where zclip itself holds focus (its own pane would
    /// otherwise be returned, and pasting into ourselves is never useful).
    fn paste_target(&self) -> Option<PaneId> {
        if let Ok((_tab, pane_id)) = get_focused_pane_info() {
            if matches!(pane_id, PaneId::Terminal(_)) {
                return Some(pane_id);
            }
        }
        self.target_pane
    }

    /// Writes a buffer into the focused terminal pane.
    ///
    /// `selector` is an optional buffer name or index; `None` means most
    /// recent, matching tmux's bare `paste-buffer`.
    fn paste(&mut self, selector: Option<&str>) {
        let Some(pane_id) = self.paste_target() else {
            self.status = Some("no terminal pane to paste into".into());
            return;
        };

        let text = match selector {
            Some(selector) => match self.ring.resolve(selector) {
                Some(buffer) => buffer.text().to_string(),
                None => {
                    self.status = Some(format!("no buffer matching '{selector}'"));
                    return;
                }
            },
            None => match self.ring.most_recent() {
                Some(buffer) => buffer.text().to_string(),
                None => {
                    self.status = Some("ring is empty — nothing to paste".into());
                    return;
                }
            },
        };

        // Fire-and-forget: the host returns nothing, so a pane that has since
        // closed is silently dropped by Zellij rather than surfacing an error.
        // That is what makes pasting into a dead pane graceful instead of fatal.
        write_chars_to_pane_id(&text, pane_id);
        self.status = Some(format!("pasted {} bytes", text.len()));
    }

    /// Renders the captured scrollback with the cursor line marked.
    fn render_copy_mode(&mut self, rows: usize, cols: usize) {
        // Reserve the header and footer lines; the rest is the text window.
        let chrome = 3;
        let height = rows.saturating_sub(chrome).max(1);

        // The pane can be resized between renders, so the viewport is
        // reconciled here against the live height rather than a stored one.
        self.last_render_height = height;

        let Some(session) = self.copy_mode.session_mut() else {
            return;
        };
        session.scroll_to_cursor(height);

        let session = &*session;
        let cursor_row = session.cursor().row;
        let total = session.scrollback().len();

        println!(
            "zclip copy mode — line {}/{} (snapshot)",
            cursor_row + 1,
            total
        );

        // Collect first: `visible_rows` borrows the session, and the highlight
        // lookup needs to borrow it again per row.
        let width = cols.saturating_sub(2);
        let visible: Vec<(usize, String)> = session
            .visible_rows(height)
            // Truncate on char boundaries; terminal columns are characters,
            // and byte-slicing here would panic on any multi-byte line.
            .map(|(row, line)| (row, line.chars().take(width).collect()))
            .collect();

        for (row, text) in visible {
            let marker = if row == cursor_row { '>' } else { ' ' };
            match session.selected_columns_for_row(row) {
                Some((start, end)) => {
                    println!("{marker} {}", highlight(&text, start, end));
                }
                None => println!("{marker} {text}"),
            }
        }

        if let Some(status) = &self.status {
            println!("  {status}");
        }
        match &self.last_key_source {
            Some((source, key)) => println!("  last key: {key} via {source}"),
            None => println!("  last key: (none yet)"),
        }
    }

    /// Renders the paste-buffer ring.
    fn render_buffer_list(&mut self, rows: usize) {
        println!(
            "zclip {} — {} buffer(s)",
            zclip_core::VERSION,
            self.ring.len()
        );
        println!();

        if self.ring.is_empty() {
            println!("  (empty) enter copy mode to capture text");
        } else {
            // Leave room for the header, blank line, status and key hints.
            let visible = rows.saturating_sub(5).max(1);
            for (index, buffer) in self.ring.iter().take(visible).enumerate() {
                let name = buffer.name().map(|n| format!(" [{n}]")).unwrap_or_default();
                println!(
                    "  {index}:{name} {} ({} line(s))",
                    buffer.preview(PREVIEW_WIDTH),
                    buffer.line_count()
                );
            }
        }

        println!();
        if let Some(status) = &self.status {
            println!("  {status}");
        }
        // No key hints: every action is bound by the user in their Zellij
        // config and dispatched over `pipe`, so this plugin has no keys of its
        // own to advertise. `y` is the one holdover, pending copy-mode yank.
        println!("  y yank (mouse selection)   d delete");
    }

    /// Routes a keypress. Only plain, unmodified keys are handled for now.
    fn handle_key(&mut self, key: &KeyWithModifier) -> bool {
        if !key.key_modifiers.is_empty() {
            return false;
        }
        match key.bare_key {
            BareKey::Char('y') => {
                self.yank();
                true
            }
            BareKey::Char('d') => {
                match self.ring.most_recent().map(|b| b.id()) {
                    Some(id) => {
                        self.ring.remove(id);
                        self.status = Some("deleted most recent buffer".into());
                    }
                    None => self.status = Some("ring is empty — nothing to delete".into()),
                }
                true
            }
            _ => false,
        }
    }
}

impl ZellijPlugin for Zclip {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        let limit = match configuration.get("buffer_limit") {
            Some(raw) => match parse_buffer_limit(raw) {
                Ok(limit) => limit,
                Err(err) => {
                    // Fall back rather than refusing to start: a typo in one
                    // config key should not render the plugin unusable.
                    self.status = Some(format!("{err}; using {DEFAULT_BUFFER_LIMIT}"));
                    DEFAULT_BUFFER_LIMIT
                }
            },
            None => DEFAULT_BUFFER_LIMIT,
        };

        self.config = configuration;
        self.ring = BufferRing::new(limit);
        // Until the first render tells us the real geometry, paging motions
        // would otherwise treat the window as zero-height and step one line.
        self.last_render_height = PROVISIONAL_HEIGHT;
        self.own_plugin_id = Some(get_plugin_ids().plugin_id);

        // `zclip-core` resolves presets and `key_*` overrides into
        // (action, raw key-spec) pairs -- it has no `zellij-tile` dependency,
        // so it cannot itself know what a `KeyWithModifier` is. Turning those
        // strings into real keys, and warning about the ones that do not
        // parse, is this crate's half of the job.
        let (bindings, mut keymap_warnings) = resolve_keymap(&self.config);
        for (action, spec) in bindings {
            let spec = spec.trim();
            // Try the whole value as ONE key first. This is what keeps a
            // literal comma bindable (`key_word_forward = ","`): splitting
            // unconditionally would turn that single-key spec into two empty
            // parts instead of the comma key itself. Only fall back to
            // splitting on commas if the whole-value parse fails -- see
            // `split_key_spec`'s doc comment for why this order matters.
            if let Ok(key) = KeyWithModifier::from_str(spec) {
                self.keys.insert(key, action);
                continue;
            }
            for part in split_key_spec(spec) {
                match KeyWithModifier::from_str(&part) {
                    Ok(key) => {
                        self.keys.insert(key, action);
                    }
                    Err(_) => keymap_warnings.push(format!(
                        "could not parse key '{part}' for action '{}', ignoring",
                        action_name(action).unwrap_or("?")
                    )),
                }
            }
        }

        // Copy mode holds the keyboard via `intercept_key_presses` -- a table
        // with no working `Cancel` binding is a trapped keyboard, not a
        // cosmetic mistake. Guarantee an escape hatch regardless of what the
        // config says, exactly as issue #29 requires.
        if !self
            .keys
            .values()
            .any(|action| *action == CopyModeAction::Cancel)
        {
            self.keys.insert(
                KeyWithModifier::from_str("Esc").expect("\"Esc\" is a valid key spec"),
                CopyModeAction::Cancel,
            );
            keymap_warnings.push(
                "no working 'cancel' binding in the resolved keymap -- reinstated Esc".into(),
            );
        }

        // Surfaced the same way `buffer_limit` errors already are, via
        // `self.status`: a malformed key table must degrade, not prevent
        // startup.
        if !keymap_warnings.is_empty() {
            let joined = keymap_warnings.join("; ");
            self.status = Some(match self.status.take() {
                Some(existing) => format!("{existing}; {joined}"),
                None => joined,
            });
        }

        request_permission(REQUIRED_PERMISSIONS);
        subscribe(&[
            EventType::PermissionRequestResult,
            EventType::PaneUpdate,
            EventType::Key,
            EventType::InterceptedKeyPress,
            EventType::BeforeClose,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::PermissionRequestResult(status) => self
                .permissions
                .resolve(status == PermissionStatus::Granted),
            Event::PaneUpdate(manifest) => {
                // Purely bookkeeping; the UI does not show the target pane, so
                // there is nothing to redraw.
                self.track_focused_pane(&manifest);
                false
            }
            Event::Key(key) => {
                if !self.ready() {
                    return false;
                }
                // Copy mode normally receives keys as InterceptedKeyPress, but
                // route this path too: it is the fallback when InterceptInput
                // is denied, and it also covers keys that reach the focused
                // plugin pane without passing through the intercept layer.
                // Zellij delivers a given press as one or the other, never
                // both, so this cannot double-handle.
                if self.copy_mode.is_active() {
                    self.last_key_source = Some(("Event::Key", key.to_string()));
                    return self.handle_copy_mode_key(&key);
                }
                self.handle_key(&key)
            }
            Event::InterceptedKeyPress(key) => {
                if !self.copy_mode.is_active() {
                    // We should not be intercepting at all in this state.
                    clear_key_presses_intercepts();
                    return false;
                }
                self.last_key_source = Some(("InterceptedKeyPress", key.to_string()));
                self.handle_copy_mode_key(&key)
            }
            Event::BeforeClose => {
                // Safety valve, same spirit as the intercept release below: if
                // zclip's pane is closed while it is standing in for a
                // terminal (mid-copy-mode), that terminal is sitting in
                // `suppressed_panes` and closing zclip without restoring it
                // would strand the user without their pane. Swap it back
                // first, before releasing the keyboard.
                if let (Some(origin), Some(own_id)) =
                    (self.copy_mode_origin.take(), self.own_plugin_id)
                {
                    replace_pane_with_existing_pane(PaneId::Plugin(own_id), origin, true);
                }
                // Safety valve: if the pane is closed while copy mode holds the
                // keyboard, release it. Otherwise the intercept outlives the UI
                // and the user is left typing into nothing.
                clear_key_presses_intercepts();
                false
            }
            _ => false,
        }
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        // Keybinding-driven commands arrive here, not through `Event::Key`:
        // `MessagePlugin` in a keybind becomes `Action::KeybindPipe` and is
        // delivered as a pipe with `PipeSource::Keybind`. That is what lets a
        // keybinding reach zclip while a *terminal* pane keeps focus, which is
        // the whole point — paste has to land in the pane you are working in.
        //
        // CLI-originated pipes (`zellij pipe`) are M5 (#36) and additionally
        // need the ReadCliPipes permission.
        if !self.ready() {
            return false;
        }

        // A CLI pipe is a *stream*: `zellij pipe --name paste -- data` delivers
        // the payload and then a trailing message with `payload: None` marking
        // end-of-stream, so a naive handler runs the command twice.
        //
        // Absence of a payload cannot be used to detect that, for two reasons:
        // a keybind pipe legitimately carries none ("paste the most recent
        // buffer"), and a bare `zellij pipe --name copy_mode` carries none
        // either — treating that as end-of-stream swallows the command
        // outright. Instead, act on the first message of each CLI pipe and
        // ignore the remainder, keyed on the pipe id the source carries.
        // Keybind pipes are one-shot and never need this.
        if let PipeSource::Cli(pipe_id) = &pipe_message.source {
            if self.last_cli_pipe.as_deref() == Some(pipe_id.as_str()) {
                return false;
            }
            self.last_cli_pipe = Some(pipe_id.clone());
        }

        let payload = pipe_message.payload.as_deref().map(str::trim);
        let selector = payload.filter(|p| !p.is_empty());

        match pipe_message.name.as_str() {
            "yank" => {
                // Stores the payload verbatim. Distinct from the `y` key path,
                // which captures a selection from a pane; this is the
                // scriptable entry point ("pipe some text straight into the
                // ring") and is what makes the pipe surface testable without a
                // mouse selection.
                match selector.and_then(|text| self.ring.push(text)) {
                    Some(_) => self.status = Some(format!("yanked ({} in ring)", self.ring.len())),
                    None => self.status = Some("nothing to yank — empty payload".into()),
                }
                true
            }
            "paste" => {
                self.paste(selector);
                true
            }
            "copy_mode" => {
                // A second press of the entry binding yanks and leaves, so the
                // one key both opens copy mode and closes it with the
                // selection taken -- the emacs-ish symmetry of hitting the
                // same key to finish what it started.
                //
                // This has to live here rather than in the copy-mode key
                // table, and that is forced rather than stylistic: the entry
                // binding is necessarily a Zellij keybinding (nothing else can
                // reach a hidden plugin), and interception only ever delivers
                // keys that are UNBOUND in the current input mode. So the
                // second press cannot arrive as `InterceptedKeyPress` the way
                // `y` does -- it arrives right back here as another pipe. See
                // issue #29.
                //
                // `yank_selection` handles an empty selection itself, with an
                // explanatory status and without exiting, so a stray
                // double-press is harmless rather than destructive.
                if self.copy_mode.is_active() {
                    self.yank_selection(false);
                    return true;
                }

                // Resolve the target BEFORE anything else: focusing ourselves
                // (or swapping ourselves into a pane) makes the host report
                // zclip as the focused pane, losing the very pane the user
                // wants to copy from.
                self.refresh_target_pane();
                // Height is unknown until render, so enter with a provisional
                // window; `render` reconciles it via scroll_to_cursor with the
                // real pane height. `enter_copy_mode` itself performs the
                // swap into the target pane's slot on success, so there is
                // nothing left to reveal here -- calling `show_self` as well
                // would fight the swap by floating zclip on top of the pane
                // it just took the place of.
                self.enter_copy_mode(PROVISIONAL_HEIGHT);
                true
            }
            "cancel" => {
                // Copy mode is exited by a user-configured Zellij binding
                // rather than a key this plugin reserves. A bound key is
                // resolved by Zellij before the intercept layer is ever
                // consulted, so this binding genuinely reaches and fires
                // during copy mode -- it is a real exit route, not merely a
                // fallback for when copy mode is inactive.
                self.exit_copy_mode();
                true
            }
            "list" => {
                // Summon the UI. `true` floats it if it was hidden.
                self.exit_copy_mode();
                show_self(true);
                true
            }
            other => {
                self.status = Some(format!("unknown command '{other}'"));
                true
            }
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        if let Some(message) = self.permissions.message() {
            println!("{message}");
            return;
        }

        if self.copy_mode.is_active() {
            self.render_copy_mode(rows, cols);
        } else {
            self.render_buffer_list(rows);
        }
    }
}

/// Wraps the inclusive character range `start..=end` of `line` in reverse video.
///
/// Works in characters rather than bytes: the columns come from the selection
/// model, which counts characters, and byte-slicing here would both mis-highlight
/// and panic on multi-byte input. Ranges beyond the end of the line are clamped,
/// which happens routinely in block selection over ragged lines.
fn highlight(line: &str, start: usize, end: usize) -> String {
    const REVERSE: &str = "\u{1b}[7m";
    // Reverse-video OFF (SGR 27), not a full reset (SGR 0). The distinction
    // is invisible today because `get_pane_scrollback` hands us colour-stripped
    // text, so there is no styling for a reset to destroy -- but SGR 0 would
    // clobber the line's own colours the moment we can capture them, turning
    // everything after a selection monochrome. tmux highlights a selection by
    // toggling reverse video over whatever attributes the cell already has,
    // and this is that same primitive.
    const REVERSE_OFF: &str = "\u{1b}[27m";

    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() || start >= chars.len() {
        return line.to_string();
    }
    let end = end.min(chars.len() - 1);

    let before: String = chars[..start].iter().collect();
    let selected: String = chars[start..=end].iter().collect();
    let after: String = chars[end + 1..].iter().collect();
    format!("{before}{REVERSE}{selected}{REVERSE_OFF}{after}")
}

register_plugin!(Zclip);
