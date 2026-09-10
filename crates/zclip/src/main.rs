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

use zclip_core::{parse_buffer_limit, BufferRing, Cursor, PermissionState, DEFAULT_BUFFER_LIMIT};
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

    /// Writes the most recent buffer into the target pane.
    fn paste(&mut self) {
        let Some(pane_id) = self.target_pane else {
            self.status = Some("no terminal pane seen yet — focus one first".into());
            return;
        };
        let Some(buffer) = self.ring.most_recent() else {
            self.status = Some("ring is empty — nothing to paste".into());
            return;
        };

        // Fire-and-forget: the host returns nothing, so a pane that has since
        // closed is silently dropped by Zellij rather than surfacing an error.
        // That is what makes pasting into a dead pane graceful instead of fatal.
        write_chars_to_pane_id(buffer.text(), pane_id);
        self.status = Some(format!("pasted {} bytes", buffer.text().len()));
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
            BareKey::Char('p') => {
                self.paste();
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

    fn pipe(&mut self, _pipe_message: PipeMessage) -> bool {
        // Driving zclip over `zellij pipe` is M5 and needs the ReadCliPipes
        // permission, which is not requested yet.
        false
    }

    fn render(&mut self, rows: usize, _cols: usize) {
        if let Some(message) = self.permissions.message() {
            println!("{message}");
            return;
        }

        println!(
            "zclip {} — {} buffer(s)",
            zclip_core::VERSION,
            self.ring.len()
        );
        println!();

        if self.ring.is_empty() {
            println!("  (empty) select text in a terminal pane, then press y");
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
        println!("  y yank   p paste   d delete");
    }
}

register_plugin!(Zclip);
