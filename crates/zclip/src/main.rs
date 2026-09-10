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

use std::collections::BTreeMap;

use zclip_core::{
    parse_buffer_limit, BufferRing, CopyMode, Cursor, PermissionState, Scrollback,
    DEFAULT_BUFFER_LIMIT,
};
use zellij_tile::prelude::*;

/// Permissions zclip requests at load time.
///
/// - `ReadApplicationState` — receive `PaneUpdate`, so we know which terminal
///   pane the user was last in (the yank source and paste target).
/// - `ReadPaneContents` — `get_pane_scrollback`, to read the selection.
/// - `WriteToStdin` — `write_chars_to_pane_id`, to paste.
///
/// Capabilities for later milestones are deliberately *not* requested, so users
/// are not asked to approve something zclip cannot yet do:
/// `InterceptInput` (copy-mode key grabbing, M2), `WriteToClipboard` /
/// `RunCommands` (clipboard bridge, M3), `ReadCliPipes` (`zellij pipe`, M5).
const REQUIRED_PERMISSIONS: &[PermissionType] = &[
    PermissionType::ReadApplicationState,
    PermissionType::ReadPaneContents,
    PermissionType::WriteToStdin,
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
    /// Our own pane id, used to exclude ourselves when hunting for the pane the
    /// user actually cares about.
    own_plugin_id: Option<u32>,
    /// The most recent *terminal* pane observed focused: both the yank source
    /// and the paste target. `None` until the first `PaneUpdate` arrives.
    target_pane: Option<PaneId>,
    /// Result of the last action, shown in the UI so that a no-op (empty
    /// selection, no target pane) explains itself instead of looking broken.
    status: Option<String>,
    /// Copy-mode lifecycle. `Inactive` means we render the buffer list.
    copy_mode: CopyMode,
    /// Id of the most recent CLI pipe handled, so the trailing end-of-stream
    /// message of that same pipe does not re-run the command. See `pipe`.
    last_cli_pipe: Option<String>,
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
        let focused = manifest
            .panes
            .values()
            .flatten()
            .find(|pane| pane.is_focused && !pane.is_plugin && !pane.exited);

        if let Some(pane) = focused {
            self.target_pane = Some(PaneId::Terminal(pane.id));
        }
    }

    /// Enters copy mode over the pane the user was last in.
    ///
    /// The scrollback is captured **once, on entry**, rather than tracked
    /// live. A pane that keeps producing output will therefore drift from what
    /// is displayed — that is a deliberate tradeoff (a moving target is
    /// unselectable), and the UI says so rather than pretending otherwise.
    fn enter_copy_mode(&mut self, height: usize) {
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
        self.copy_mode.enter(scrollback, height);
        self.status = Some(format!("copy mode — {lines} line(s) captured"));
    }

    /// Leaves copy mode, returning to the buffer list.
    fn exit_copy_mode(&mut self) {
        if self.copy_mode.exit() {
            self.status = Some("left copy mode".into());
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

        for (row, line) in session.visible_rows(height) {
            let marker = if row == cursor_row { '>' } else { ' ' };
            // Truncate on char boundaries; terminal columns are characters,
            // and byte-slicing here would panic on any multi-byte line.
            let width = cols.saturating_sub(2);
            let text: String = line.chars().take(width).collect();
            println!("{marker} {text}");
        }

        if let Some(status) = &self.status {
            println!("  {status}");
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
        self.own_plugin_id = Some(get_plugin_ids().plugin_id);

        request_permission(REQUIRED_PERMISSIONS);
        subscribe(&[
            EventType::PermissionRequestResult,
            EventType::PaneUpdate,
            EventType::Key,
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
                self.handle_key(&key)
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
                // Height is unknown until render, so enter with a provisional
                // window; `render` reconciles it via scroll_to_cursor with the
                // real pane height.
                show_self(true);
                self.enter_copy_mode(PROVISIONAL_HEIGHT);
                true
            }
            "cancel" => {
                // Copy mode is exited by a user-configured Zellij binding
                // rather than a key this plugin reserves. While the plugin
                // pane is focused Zellij consumes bound keys before they ever
                // reach `Event::Key`, so hardcoding Escape here would either
                // be shadowed by the user's config or fight it.
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

register_plugin!(Zclip);
