//! The tmux-style copy-mode state machine: lifecycle and the scroll viewport.
//!
//! **Cursor motions and text selection are explicitly out of scope here** —
//! those are separate, later pieces of work. This module owns only two
//! things: whether copy mode is active at all, and, while it is, which
//! window of the captured [`Scrollback`] is currently visible on screen. A
//! future motion layer will call [`CopySession::set_cursor`] to move the
//! cursor around; this module guarantees that doing so always keeps the
//! viewport and the cursor's position consistent with each other.

use crate::scrollback::Scrollback;
use crate::selection::Cursor;

/// Whether copy mode is currently active, and if so, its session state.
///
/// Modelled as an enum rather than a `bool` plus an `Option<CopySession>`
/// so that "active" and "has a session" can never disagree with each other.
#[derive(Debug, Clone)]
pub enum CopyMode {
    /// Copy mode is off. There is no captured scrollback or cursor to speak
    /// of.
    Inactive,
    /// Copy mode is on, with the state captured in the session.
    Active(CopySession),
}

impl Default for CopyMode {
    /// Copy mode starts off, matching how tmux panes behave before the user
    /// explicitly enters copy mode.
    fn default() -> Self {
        Self::Inactive
    }
}

impl CopyMode {
    /// Whether copy mode is currently active.
    #[must_use]
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active(_))
    }

    /// The current session, if copy mode is active.
    #[must_use]
    pub fn session(&self) -> Option<&CopySession> {
        match self {
            Self::Active(session) => Some(session),
            Self::Inactive => None,
        }
    }

    /// A mutable reference to the current session, if copy mode is active.
    ///
    /// This is how a future motion layer will reach [`CopySession::set_cursor`]
    /// without this module needing to expose every motion as its own method.
    #[must_use]
    pub fn session_mut(&mut self) -> Option<&mut CopySession> {
        match self {
            Self::Active(session) => Some(session),
            Self::Inactive => None,
        }
    }

    /// Enters copy mode, capturing `scrollback` as the snapshot to browse.
    ///
    /// If copy mode is already active, the existing session is **replaced**
    /// entirely: entering again re-captures from scratch, discarding the old
    /// cursor position and scroll offset. This matches the expected
    /// behaviour of exiting and re-entering copy mode (e.g. after leaving to
    /// look at something else) rather than trying to preserve a position
    /// that may no longer make sense against fresh content.
    pub fn enter(&mut self, scrollback: Scrollback, height: usize) {
        *self = Self::Active(CopySession::new(scrollback, height));
    }

    /// Exits copy mode.
    ///
    /// Returns whether this actually changed anything, so callers can tell
    /// whether a re-render is needed: exiting while already inactive is a
    /// no-op that returns `false`.
    pub fn exit(&mut self) -> bool {
        if matches!(self, Self::Inactive) {
            return false;
        }
        *self = Self::Inactive;
        true
    }
}

/// The state of a single copy-mode session: the captured text, the cursor's
/// position within it, and which window of it is currently scrolled into
/// view.
///
/// # Why `height` is never stored
///
/// Every method here that needs to know how many rows are on screen takes
/// `height` as a parameter, rather than this struct remembering it from when
/// the session was created. A pane can be resized while copy mode is active,
/// and a `height` cached from session creation would silently go stale the
/// moment that happens — the viewport math would keep computing against a
/// window size that no longer matches reality. Requiring the caller to pass
/// the *current* height on every call makes that impossible: there is no
/// stored value to forget to update.
#[derive(Debug, Clone)]
pub struct CopySession {
    scrollback: Scrollback,
    cursor: Cursor,
    top: usize,
}

impl CopySession {
    /// Starts a new session over `scrollback`.
    ///
    /// The cursor starts at the **first viewport row**, column `0` — not at
    /// row `0` of the whole snapshot. When the user enters copy mode, their
    /// attention is on what they were just looking at on screen, not on the
    /// oldest line of scrollback history that happens to have been
    /// captured; starting the cursor anywhere else would put it somewhere
    /// the user never asked to look at. `top` is then chosen so that
    /// starting position is immediately visible.
    #[must_use]
    pub fn new(scrollback: Scrollback, height: usize) -> Self {
        let start_row = scrollback.viewport_start();
        let cursor = scrollback.clamp_cursor(Cursor::new(start_row, 0));
        let mut session = Self {
            scrollback,
            cursor,
            top: 0,
        };
        session.scroll_to_cursor(height);
        session
    }

    /// The captured snapshot this session is browsing.
    #[must_use]
    pub fn scrollback(&self) -> &Scrollback {
        &self.scrollback
    }

    /// The cursor's current position, as an absolute row/col into
    /// [`Self::scrollback`].
    #[must_use]
    pub fn cursor(&self) -> Cursor {
        self.cursor
    }

    /// Moves the cursor.
    ///
    /// `cursor` is clamped via [`Scrollback::clamp_cursor`] first, so a
    /// motion computed against stale bounds can never park the cursor past
    /// the end of a line or past the end of the snapshot. This is the single
    /// funnel every cursor movement (a later motions module) is expected to
    /// go through, which is what keeps the clamp invariant from being
    /// bypassed by some other code path setting the field directly.
    ///
    /// This deliberately does **not** scroll. Keeping the cursor visible
    /// requires knowing the pane height, which is only reliably known at
    /// render time (the pane can be resized between renders, so storing it
    /// here would be stale). Scrolling is therefore
    /// [`Self::scroll_to_cursor`]'s job, called with the live height.
    ///
    /// Resist the temptation to scroll "just upward" here on the grounds that
    /// it needs no height: that would leave the cursor-visible invariant
    /// holding in one direction and not the other, which is harder to reason
    /// about than it plainly not holding at all. The rule is simply: move the
    /// cursor here, reconcile the viewport at render.
    pub fn set_cursor(&mut self, cursor: Cursor) {
        self.cursor = self.scrollback.clamp_cursor(cursor);
    }

    /// The absolute row index of the first line currently scrolled into
    /// view.
    #[must_use]
    pub fn top(&self) -> usize {
        self.top
    }

    /// Adjusts `top` by the minimum amount needed so the cursor's row falls
    /// within `[top, top + height)`.
    ///
    /// If the cursor is above the current window, `top` becomes exactly the
    /// cursor's row (scroll up just enough to reveal it). If the cursor is
    /// at or below the bottom of the window, `top` becomes
    /// `cursor.row + 1 - height` (scroll down just enough that the cursor
    /// becomes the last visible row). If the cursor is already visible,
    /// `top` is left untouched.
    ///
    /// `height` of `0` is treated as `1`: a zero-height render is a
    /// degenerate case a caller might pass during startup or a resize
    /// glitch, and this must not divide by zero or otherwise panic.
    pub fn scroll_to_cursor(&mut self, height: usize) {
        let height = height.max(1);
        if self.cursor.row < self.top {
            self.top = self.cursor.row;
        } else if self.cursor.row >= self.top + height {
            self.top = self.cursor.row + 1 - height;
        }
    }

    /// The lines currently visible in a viewport of `height` rows, as
    /// `(absolute_row_index, line)` pairs.
    ///
    /// The absolute index is yielded alongside each line so a renderer can
    /// tell which row (if any) holds the cursor without separately
    /// recomputing offsets from `top`. Iteration stops at the end of the
    /// scrollback even if `height` would otherwise ask for more rows than
    /// exist, so a caller never needs to check bounds itself.
    ///
    /// `height` of `0` is treated as `1`, for the same reason as
    /// [`Self::scroll_to_cursor`].
    pub fn visible_rows(&self, height: usize) -> impl Iterator<Item = (usize, &str)> + '_ {
        let height = height.max(1);
        let end = self.scrollback.len().min(self.top.saturating_add(height));
        (self.top..end).filter_map(move |row| self.scrollback.line(row).map(|line| (row, line)))
    }

    /// The cursor's row relative to `top`, i.e. which on-screen line should
    /// be highlighted, or `None` if the cursor is not currently within the
    /// visible window.
    ///
    /// `height` of `0` is treated as `1`, for the same reason as
    /// [`Self::scroll_to_cursor`].
    #[must_use]
    pub fn cursor_screen_row(&self, height: usize) -> Option<usize> {
        let height = height.max(1);
        if self.cursor.row < self.top || self.cursor.row >= self.top + height {
            return None;
        }
        Some(self.cursor.row - self.top)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn entering_makes_copy_mode_active() {
        let mut mode = CopyMode::default();
        assert!(!mode.is_active());

        mode.enter(Scrollback::empty(), 10);

        assert!(mode.is_active());
        assert!(mode.session().is_some());
    }

    #[test]
    fn exit_returns_true_once_then_false_on_a_second_call() {
        let mut mode = CopyMode::default();
        mode.enter(Scrollback::empty(), 10);

        assert!(mode.exit(), "first exit actually changed state");
        assert!(!mode.exit(), "already inactive, nothing changed");
        assert!(!mode.is_active());
    }

    #[test]
    fn exit_while_already_inactive_returns_false() {
        let mut mode = CopyMode::default();
        assert!(!mode.exit());
    }

    #[test]
    fn re_entering_replaces_the_existing_session() {
        let mut mode = CopyMode::default();
        mode.enter(
            Scrollback::from_parts(vec![], strings(&["first", "session"]), vec![]),
            10,
        );
        mode.session_mut().unwrap().set_cursor(Cursor::new(1, 3));

        mode.enter(
            Scrollback::from_parts(vec![], strings(&["second", "session"]), vec![]),
            10,
        );

        let session = mode.session().unwrap();
        assert_eq!(
            session.cursor(),
            Cursor::new(0, 0),
            "re-entering must reset the cursor to the new snapshot's viewport start"
        );
        assert_eq!(session.scrollback().line(0), Some("second"));
    }

    #[test]
    fn cursor_starts_at_the_first_viewport_row_not_row_zero_when_there_is_history_above() {
        let scrollback =
            Scrollback::from_parts(strings(&["old 1", "old 2"]), strings(&["view 1"]), vec![]);

        let session = CopySession::new(scrollback, 10);

        assert_eq!(session.cursor(), Cursor::new(2, 0));
    }

    #[test]
    fn scroll_to_cursor_scrolls_up_when_cursor_is_above_the_window() {
        let scrollback =
            Scrollback::from_parts(vec![], strings(&["a", "b", "c", "d", "e"]), vec![]);
        let mut session = CopySession::new(scrollback, 2);
        session.top = 3;
        session.cursor = Cursor::new(0, 0);

        session.scroll_to_cursor(2);

        assert_eq!(session.top(), 0, "scrolling up to reveal row 0");
    }

    #[test]
    fn scroll_to_cursor_scrolls_down_when_cursor_is_below_the_window() {
        let scrollback =
            Scrollback::from_parts(vec![], strings(&["a", "b", "c", "d", "e"]), vec![]);
        let mut session = CopySession::new(scrollback, 2);
        session.top = 0;
        session.cursor = Cursor::new(4, 0);

        session.scroll_to_cursor(2);

        assert_eq!(
            session.top(),
            3,
            "top becomes cursor.row + 1 - height so the cursor is the last visible row"
        );
    }

    #[test]
    fn scroll_to_cursor_leaves_top_unchanged_when_cursor_already_visible() {
        let scrollback =
            Scrollback::from_parts(vec![], strings(&["a", "b", "c", "d", "e"]), vec![]);
        let mut session = CopySession::new(scrollback, 3);
        session.top = 1;
        session.cursor = Cursor::new(2, 0);

        session.scroll_to_cursor(3);

        assert_eq!(session.top(), 1);
    }

    #[test]
    fn visible_rows_yields_absolute_indices_and_stops_at_the_end_of_the_scrollback() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["a", "b", "c"]), vec![]);
        let session = CopySession::new(scrollback, 10);

        let rows: Vec<(usize, &str)> = session.visible_rows(10).collect();

        assert_eq!(rows, vec![(0, "a"), (1, "b"), (2, "c")]);
    }

    #[test]
    fn visible_rows_with_height_larger_than_the_scrollback_does_not_repeat_or_panic() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["only"]), vec![]);
        let session = CopySession::new(scrollback, 50);

        let rows: Vec<(usize, &str)> = session.visible_rows(50).collect();

        assert_eq!(rows, vec![(0, "only")]);
    }

    #[test]
    fn cursor_screen_row_returns_the_offset_from_top() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["a", "b", "c", "d"]), vec![]);
        let mut session = CopySession::new(scrollback, 2);
        session.top = 1;
        session.cursor = Cursor::new(2, 0);

        assert_eq!(session.cursor_screen_row(2), Some(1));
    }

    #[test]
    fn cursor_screen_row_returns_none_when_the_cursor_has_scrolled_away() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["a", "b", "c", "d"]), vec![]);
        let mut session = CopySession::new(scrollback, 2);
        session.top = 0;
        session.cursor = Cursor::new(3, 0);

        assert_eq!(session.cursor_screen_row(2), None);
    }

    #[test]
    fn set_cursor_clamps_an_out_of_range_row_and_col() {
        let scrollback =
            Scrollback::from_parts(vec![], strings(&["short", "a longer line"]), vec![]);
        let mut session = CopySession::new(scrollback, 10);

        session.set_cursor(Cursor::new(99, 99));

        assert_eq!(session.cursor(), Cursor::new(1, 13));
    }

    #[test]
    fn set_cursor_never_scrolls_in_either_direction() {
        // The viewport is reconciled at render time by scroll_to_cursor, which
        // is the only place the live pane height is known. set_cursor moving
        // `top` even "just upward" would make the cursor-visible invariant hold
        // asymmetrically, which is worse than it not holding here at all.
        let above = strings(&["h0", "h1", "h2", "h3", "h4"]);
        let scrollback = Scrollback::from_parts(above, strings(&["v0", "v1", "v2"]), vec![]);
        let mut session = CopySession::new(scrollback, 3);
        let top_on_entry = session.top();

        session.set_cursor(Cursor::new(0, 0)); // far above the window
        assert_eq!(session.top(), top_on_entry, "must not scroll up");

        session.set_cursor(Cursor::new(7, 0)); // far below the window
        assert_eq!(session.top(), top_on_entry, "must not scroll down");

        // ...and the render pass reconciles it.
        session.scroll_to_cursor(3);
        assert_eq!(session.cursor_screen_row(3), Some(2));
    }

    #[test]
    fn zero_height_does_not_panic_in_visible_rows() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["a", "b"]), vec![]);
        let session = CopySession::new(scrollback, 0);

        let rows: Vec<(usize, &str)> = session.visible_rows(0).collect();

        assert_eq!(rows, vec![(0, "a")]);
    }

    #[test]
    fn zero_height_does_not_panic_in_scroll_to_cursor() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["a", "b", "c"]), vec![]);
        let mut session = CopySession::new(scrollback, 0);
        session.set_cursor(Cursor::new(2, 0));

        session.scroll_to_cursor(0);

        assert_eq!(session.top(), 2);
    }

    #[test]
    fn zero_height_does_not_panic_in_cursor_screen_row() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["a"]), vec![]);
        let session = CopySession::new(scrollback, 0);

        assert_eq!(session.cursor_screen_row(0), Some(0));
    }

    #[test]
    fn empty_scrollback_enter_then_visible_rows_yields_nothing_and_cursor_is_origin() {
        let mut mode = CopyMode::default();

        mode.enter(Scrollback::empty(), 10);

        let session = mode.session().unwrap();
        assert_eq!(session.cursor(), Cursor::new(0, 0));
        assert_eq!(session.visible_rows(10).count(), 0);
        assert_eq!(session.cursor_screen_row(10), Some(0));
    }
}
