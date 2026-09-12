//! Host-independent core logic for [zclip](https://github.com/dev-pmallapp/zclip).
//!
//! This crate deliberately has **no dependencies**. It knows nothing about
//! Zellij, WASM, or the host API; it only models zclip's data and behaviour:
//! the paste-buffer ring, selections, search and filtering.
//!
//! Keeping that logic here means `cargo test -p zclip-core` runs on any target
//! in seconds, with no system libraries and no WASM toolchain. The
//! Zellij-facing shim lives in the sibling `zclip` crate.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod buffer;
pub mod copy_mode;
pub mod keymap;
pub mod motion;
pub mod permissions;
pub mod persist;
pub mod region;
pub mod scrollback;
pub mod selection;

pub use buffer::{
    parse_buffer_limit, BufferId, BufferLimitError, BufferRing, PasteBuffer, DEFAULT_BUFFER_LIMIT,
};
pub use copy_mode::{CopyMode, CopySession};
pub use keymap::{
    action_by_name, action_name, resolve_keymap, split_key_spec, CopyModeAction, EMACS_PRESET,
    VI_PRESET,
};
pub use motion::{apply as apply_motion, Motion};
pub use permissions::PermissionState;
pub use persist::{parse_persist_mode, PersistMode};
pub use region::{extract_region, selected_columns, SelectionMode};
pub use scrollback::Scrollback;
pub use selection::{extract_span, trim_trailing_blanks, Cursor};

/// The version of the zclip core library, as reported to users and logs.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_populated() {
        assert!(!VERSION.is_empty());
        assert!(VERSION.starts_with(char::is_numeric));
    }
}
