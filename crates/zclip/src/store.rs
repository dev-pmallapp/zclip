//! Host glue for [`persist`](zclip_core::persist): the only place in this
//! repo that touches `std::fs`.
//!
//! `zclip-core::persist` only knows how to turn a [`BufferRing`] into a
//! `String` and back, plus the naming/staleness policy -- it is a
//! WASI-sandbox binary as much as a native one, so *where* to write and
//! *when* those bytes actually hit disk is deliberately left to this crate.
//! See the module docs there for the full rationale.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use zclip_core::persist::{self, PersistMode};
use zclip_core::BufferRing;
use zellij_tile::prelude::PluginIds;

/// Where every persisted ring file lives.
///
/// A subdirectory zclip owns exclusively, not `/cache` itself: `gc` lists
/// this directory and deletes anything in it that looks stale, and
/// enumerating a directory the host (or some other plugin) also writes
/// into is how you eventually delete something that is not yours.
const CACHE_SUBDIR: &str = "zclip";

/// Reads and writes a single plugin instance's persisted paste-buffer ring.
///
/// `None` when persistence is off, so the caller never makes a filesystem
/// call at all -- a user who never opted in should see zero `/cache`
/// activity, not a store that happens to no-op.
pub struct RingStore {
    /// `/cache/zclip`, computed once so every method agrees on it.
    dir: PathBuf,
    /// This instance's ring file, keyed on `zellij_pid` -- the Zellij
    /// *server* process, one per session -- rather than `plugin_id`. That
    /// is what gives the ring tmux's contract: it survives plugin reload,
    /// pane close, and detach/attach, and dies only when the session
    /// (server) itself does.
    path: PathBuf,
    /// The temp file `save` writes to before renaming into place, keyed on
    /// `plugin_id` rather than `zellij_pid` so that two zclip instances
    /// alive in the same session at once (e.g. mid-reload) can never
    /// collide on the same temp path.
    tmp: PathBuf,
}

impl RingStore {
    /// Builds a store for `mode`, or `None` if persistence is off.
    pub fn new(mode: PersistMode, ids: &PluginIds) -> Option<Self> {
        if mode == PersistMode::Off {
            return None;
        }
        let dir = cache_dir();
        let path = dir.join(persist::ring_file_name(ids.zellij_pid));
        let tmp = dir.join(persist::temp_file_name(ids.plugin_id));
        Some(Self { dir, path, tmp })
    }

    /// Restores this instance's ring, if a file exists, and reports what
    /// happened in a form suitable for `self.status`.
    ///
    /// A missing file produces no message: that is simply the normal first
    /// run (or first run after the last session's file was GC'd), and
    /// must not read like an error to a user who has done nothing wrong.
    pub fn load(&self, limit: usize) -> (BufferRing, Option<String>) {
        let body = match fs::read_to_string(&self.path) {
            Ok(body) => body,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return (BufferRing::new(limit), None);
            }
            Err(err) => {
                return (
                    BufferRing::new(limit),
                    Some(format!("could not read persisted ring: {err}")),
                );
            }
        };

        match persist::decode(&body, limit) {
            Ok(restored) => {
                // Visible feedback on a successful restore is not optional
                // decoration: persistence is opt-in, and a user's consent to
                // "write my clipboard history to disk" is only meaningful if
                // they can actually see it doing something.
                let mut message = format!("restored {} buffer(s)", restored.ring.len());
                if restored.skipped_records > 0 {
                    // A partial restore beats total loss, but silently
                    // dropping records would hide that the file was
                    // damaged in the first place.
                    message.push_str(&format!(
                        " ({} record(s) could not be restored)",
                        restored.skipped_records
                    ));
                }
                (restored.ring, Some(message))
            }
            Err(err) => (
                BufferRing::new(limit),
                Some(format!("could not restore persisted ring: {err}")),
            ),
        }
    }

    /// Writes `ring` to disk, atomically.
    ///
    /// An empty ring deletes the file instead of writing a header-only one:
    /// leaving the old file behind after the ring has actually emptied
    /// would make "everything was deleted" a lie the next restore tells.
    ///
    /// Otherwise the encoded body is written to `tmp` and then renamed onto
    /// `path`. The rename is within one directory, hence atomic on any
    /// filesystem that matters here, so a reader can only ever see either
    /// the whole old file or the whole new one -- never a torn write. No
    /// `sync_all`: this is a cache the user opted into, not a ledger, and
    /// losing the last couple of seconds of it to a crash is an acceptable
    /// tradeoff for not fsyncing on every yank.
    pub fn save(&self, ring: &BufferRing) -> Result<(), String> {
        if ring.is_empty() {
            return match fs::remove_file(&self.path) {
                Ok(()) => Ok(()),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(err) => Err(format!("could not remove persisted ring: {err}")),
            };
        }

        fs::create_dir_all(&self.dir)
            .map_err(|err| format!("could not create persist directory: {err}"))?;

        let encoded = persist::encode(ring);
        fs::write(&self.tmp, encoded.body)
            .map_err(|err| format!("could not write persisted ring: {err}"))?;
        fs::rename(&self.tmp, &self.path)
            .map_err(|err| format!("could not finalize persisted ring: {err}"))?;
        Ok(())
    }

    /// Best-effort removal of other instances' abandoned ring files.
    ///
    /// Zellij server pids get recycled by the OS over time, so without a
    /// TTL a brand-new session that happens to draw a pid a long-dead
    /// session once used would silently "restore" that dead session's
    /// buffers as its own. Every failure here -- an unreadable directory,
    /// an unreadable entry, a failed delete -- is swallowed: garbage
    /// collection is a nicety, and a user must never see a warning about a
    /// cleanup step that has nothing to do with the ring they actually
    /// care about.
    pub fn gc(&self) {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path == self.path {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !persist::is_ring_file(name) {
                continue;
            }
            let Ok(modified) = entry.metadata().and_then(|metadata| metadata.modified()) else {
                continue;
            };
            // An `Err` here means `modified` is in the future relative to
            // now (clock skew, or a filesystem with a wildly wrong clock)
            // -- treat that as "not stale" rather than erroring, since
            // there is nothing safe to delete in that ambiguity.
            let Ok(age) = SystemTime::now().duration_since(modified) else {
                continue;
            };
            if persist::is_stale(age.as_secs()) {
                let _ = fs::remove_file(&path);
            }
        }
    }
}

/// `/cache/zclip`, the directory the plugin sandbox grants writable and
/// that Zellij never cleans up on its own (unlike `/data`, which is
/// `remove_dir_all`'d on every plugin unload and therefore cannot be used
/// for anything meant to survive a reload).
fn cache_dir() -> PathBuf {
    Path::new("/cache").join(CACHE_SUBDIR)
}
