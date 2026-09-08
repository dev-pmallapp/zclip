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

use zclip_core::PermissionState;
use zellij_tile::prelude::*;

/// Permissions zclip requests at load time.
///
/// Deliberately the minimum needed to function:
///
/// - `ReadApplicationState` — enumerate tabs/panes so we know where to paste.
/// - `WriteToStdin` — write buffer contents into the target pane.
///
/// Capabilities for later milestones are *not* requested here, so that users
/// are not asked to approve something zclip cannot yet do. They are added
/// alongside the features that need them:
///
/// - `ReadPaneContents` — copy mode reading scrollback (M2).
/// - `InterceptInput` — copy-mode key interception (M2).
/// - `WriteToClipboard` / `RunCommands` — the system clipboard bridge (M3).
/// - `ReadCliPipes` — `zellij pipe` integration (M5).
const REQUIRED_PERMISSIONS: &[PermissionType] = &[
    PermissionType::ReadApplicationState,
    PermissionType::WriteToStdin,
];

/// Top-level plugin state.
///
/// Feature state (buffer ring, copy mode, UI) is added in later milestones; for
/// now this carries the configuration handed to us by Zellij and tracks whether
/// the host has granted [`REQUIRED_PERMISSIONS`].
#[derive(Debug, Default)]
pub struct Zclip {
    /// Raw key/value configuration parsed from the plugin's KDL block.
    config: BTreeMap<String, String>,
    /// Gate for every permission-requiring host call. See [`Zclip::ready`].
    permissions: PermissionState,
}

impl Zclip {
    /// Whether gated host calls (pane reads, stdin writes, clipboard access)
    /// may be made.
    ///
    /// Every call site that touches a gated API must check this first;
    /// calling early is a silent no-op in Zellij, not an error.
    fn ready(&self) -> bool {
        self.permissions.is_ready()
    }
}

impl ZellijPlugin for Zclip {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.config = configuration;

        request_permission(REQUIRED_PERMISSIONS);
        subscribe(&[EventType::PermissionRequestResult]);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::PermissionRequestResult(status) => self
                .permissions
                .resolve(status == PermissionStatus::Granted),
            _ => false,
        }
    }

    fn pipe(&mut self, _pipe_message: PipeMessage) -> bool {
        // Nothing is driveable from a pipe yet, and doing so would need
        // permissions we may not have. Bail out until the gate opens.
        if !self.ready() {
            return false;
        }

        false
    }

    fn render(&mut self, _rows: usize, _cols: usize) {
        // Pending/denied states get an explanation rather than an empty pane,
        // which would otherwise look like the plugin had simply failed.
        if let Some(message) = self.permissions.message() {
            println!("{message}");
            return;
        }

        println!(
            "zclip {} — scaffold loaded ({} config keys)",
            zclip_core::VERSION,
            self.config.len()
        );
    }
}

register_plugin!(Zclip);
