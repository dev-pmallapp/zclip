//! Permission gating state.
//!
//! Zellij plugins must request capabilities (reading pane contents, writing to
//! stdin, running commands) and wait for the user to approve them before any
//! gated host call will do anything. Calling a gated API early is a silent
//! no-op, which is painful to debug — so zclip funnels every gated call through
//! [`PermissionState`].
//!
//! This is modelled as a three-state machine rather than a `bool` so that the
//! UI can distinguish "waiting for you to answer the prompt" from "you said
//! no", which need very different messages.

use core::fmt;

/// Whether the host has granted the permissions zclip asked for at load time.
///
/// Starts at [`PermissionState::Pending`] and moves exactly once, in response
/// to Zellij's permission-result event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PermissionState {
    /// The request has been sent and the user has not answered yet.
    #[default]
    Pending,
    /// The user approved the request. Gated host calls are safe to make.
    Granted,
    /// The user declined. zclip cannot function and should say so.
    Denied,
}

impl PermissionState {
    /// Whether gated host calls may be made.
    ///
    /// This is the only predicate callers should branch on before touching a
    /// gated API.
    #[must_use]
    pub fn is_ready(self) -> bool {
        matches!(self, Self::Granted)
    }

    /// Whether zclip is in a terminal state that the user must act on.
    #[must_use]
    pub fn is_denied(self) -> bool {
        matches!(self, Self::Denied)
    }

    /// Record the host's answer.
    ///
    /// Returns `true` if this actually changed the state, which the caller
    /// should propagate as "a re-render is needed". Repeated identical results
    /// return `false` so we do not redraw for nothing.
    pub fn resolve(&mut self, granted: bool) -> bool {
        let next = if granted { Self::Granted } else { Self::Denied };
        let changed = *self != next;
        *self = next;
        changed
    }

    /// A short human-readable explanation, shown when zclip cannot yet work.
    ///
    /// Returns `None` once permissions are granted, because at that point the
    /// plugin should be rendering its real UI instead.
    #[must_use]
    pub fn message(self) -> Option<&'static str> {
        match self {
            Self::Pending => Some("zclip: waiting for permission — accept the prompt to continue."),
            Self::Denied => Some(
                "zclip: permissions denied. zclip cannot read or write panes. \
                 Reload the plugin to be asked again.",
            ),
            Self::Granted => None,
        }
    }
}

impl fmt::Display for PermissionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Pending => "pending",
            Self::Granted => "granted",
            Self::Denied => "denied",
        };
        f.write_str(label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_pending_and_not_ready() {
        let state = PermissionState::default();

        assert_eq!(state, PermissionState::Pending);
        assert!(!state.is_ready());
        assert!(!state.is_denied());
    }

    #[test]
    fn granting_makes_it_ready() {
        let mut state = PermissionState::default();

        assert!(
            state.resolve(true),
            "first resolution should request a render"
        );
        assert!(state.is_ready());
        assert_eq!(state.message(), None);
    }

    #[test]
    fn denying_is_reported_but_never_ready() {
        let mut state = PermissionState::default();

        assert!(state.resolve(false));
        assert!(!state.is_ready());
        assert!(state.is_denied());
        assert!(state.message().is_some());
    }

    #[test]
    fn repeated_identical_results_do_not_request_a_render() {
        let mut state = PermissionState::default();

        assert!(state.resolve(true));
        assert!(!state.resolve(true), "no state change, so no re-render");
        assert!(state.is_ready());
    }

    #[test]
    fn a_later_denial_revokes_readiness() {
        let mut state = PermissionState::default();
        state.resolve(true);

        assert!(state.resolve(false), "granted -> denied is a change");
        assert!(!state.is_ready(), "must never stay ready after a denial");
    }

    #[test]
    fn pending_and_denied_have_distinct_messages() {
        let pending = PermissionState::Pending.message();
        let denied = PermissionState::Denied.message();

        assert!(pending.is_some() && denied.is_some());
        assert_ne!(pending, denied);
    }
}
