//! The tmux-style copy-mode state machine: lifecycle, the scroll viewport,
//! and the current selection.
//!
//! **Cursor motion itself is explicitly out of scope here** — computing
//! *where* the cursor should land for a given vi motion lives in
//! [`crate::motion`], and callers reach this module's cursor through
//! [`CopySession::set_cursor`]. This module guarantees that moving the
//! cursor always keeps the viewport, the cursor's position, and any active
//! selection consistent with each other.
//!
//! It does, however, own the selection's *shape and anchor*: whether a
//! selection is active, which of tmux's char/line/block shapes
//! ([`SelectionMode`]) it uses, and where it was started. Turning that
//! anchor-plus-cursor pair into actual text or a highlighted column range is
//! [`crate::region`]'s job; this module just tracks the state and delegates
//! to it.

use crate::region::{self, SelectionMode};
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
    selection_anchor: Option<Cursor>,
    selection_mode: SelectionMode,
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
    ///
    /// There is no selection yet: entering copy mode is not the same as
    /// starting to select, and a fresh session should never appear to have
    /// text highlighted before the user has asked for any.
    #[must_use]
    pub fn new(scrollback: Scrollback, height: usize) -> Self {
        let start_row = scrollback.viewport_start();
        let cursor = scrollback.clamp_cursor(Cursor::new(start_row, 0));
        let mut session = Self {
            scrollback,
            cursor,
            top: 0,
            selection_anchor: None,
            selection_mode: SelectionMode::Char,
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

    /// Starts a new selection, anchored at the cursor's **current**
    /// position, using `mode` as its shape.
    ///
    /// The anchor is captured now, in the same absolute row space as the
    /// cursor (see [`crate::region`]'s module docs on why that matters), so
    /// it stays meaningful even as the viewport scrolls or the cursor moves
    /// on afterward. This is the `v` / `V` / `Ctrl-v` entry point: whatever
    /// selection existed before is discarded and replaced.
    pub fn start_selection(&mut self, mode: SelectionMode) {
        self.selection_anchor = Some(self.cursor);
        self.selection_mode = mode;
    }

    /// Changes the active selection's shape to `mode` **without** moving
    /// its anchor.
    ///
    /// If no selection is active yet, this starts one at the cursor instead
    /// (matching [`Self::start_selection`]), so a caller never needs to
    /// check [`Self::has_selection`] first just to switch shapes. This is
    /// what makes pressing `v` then `V` behave like vi: the first press
    /// starts a char-wise selection, the second switches it to line-wise in
    /// place, keeping wherever the user first started selecting from.
    pub fn set_selection_mode(&mut self, mode: SelectionMode) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor);
        }
        self.selection_mode = mode;
    }

    /// Toggles a selection in `mode`: if a selection is already active in
    /// that **same** mode, it is cleared; otherwise a selection in `mode`
    /// is started (or switched to) at the cursor's current position.
    ///
    /// This models vi's `v` (or `V`, or `Ctrl-v`) pressed twice in a row
    /// cancelling the selection it just started, while pressing a
    /// *different* selection key switches shape instead of cancelling —
    /// the same distinction [`Self::set_selection_mode`] draws.
    pub fn toggle_selection(&mut self, mode: SelectionMode) {
        if self.selection_anchor.is_some() && self.selection_mode == mode {
            self.clear_selection();
        } else {
            self.start_selection(mode);
        }
    }

    /// Clears the active selection, if any.
    pub fn clear_selection(&mut self) {
        self.selection_anchor = None;
    }

    /// The selection's anchor point, or `None` if no selection is active.
    ///
    /// Like the cursor, this is an absolute row/col position into
    /// [`Self::scrollback`], not a position relative to the current
    /// viewport.
    #[must_use]
    pub fn selection_anchor(&self) -> Option<Cursor> {
        self.selection_anchor
    }

    /// The active selection's shape.
    ///
    /// This is meaningful even with no selection active (it simply reports
    /// whatever shape the next selection would start in), so callers can
    /// always read it without matching on [`Self::has_selection`] first.
    #[must_use]
    pub fn selection_mode(&self) -> SelectionMode {
        self.selection_mode
    }

    /// Whether a selection is currently active.
    #[must_use]
    pub fn has_selection(&self) -> bool {
        self.selection_anchor.is_some()
    }

    /// The text currently selected, or `None` if no selection is active.
    ///
    /// Delegates to [`region::extract_region`] over this session's own
    /// scrollback, anchor, and cursor, so this is always exactly what
    /// [`Self::selected_columns_for_row`] highlights.
    #[must_use]
    pub fn selected_text(&self) -> Option<String> {
        let anchor = self.selection_anchor?;
        Some(region::extract_region(
            &self.scrollback,
            anchor,
            self.cursor,
            self.selection_mode,
        ))
    }

    /// The inclusive `(start_col, end_col)` range of `row` that is
    /// currently selected, or `None` if no selection is active or `row`
    /// falls outside it.
    ///
    /// Delegates to [`region::selected_columns`], using this session's own
    /// scrollback to look up `row`'s character width.
    #[must_use]
    pub fn selected_columns_for_row(&self, row: usize) -> Option<(usize, usize)> {
        let anchor = self.selection_anchor?;
        let width = self.scrollback.line_width(row);
        region::selected_columns(row, anchor, self.cursor, self.selection_mode, width)
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

    #[test]
    fn start_selection_anchors_at_the_current_cursor_position() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["hello world"]), vec![]);
        let mut session = CopySession::new(scrollback, 10);
        session.set_cursor(Cursor::new(0, 3));

        session.start_selection(SelectionMode::Char);

        assert_eq!(session.selection_anchor(), Some(Cursor::new(0, 3)));
        assert!(session.has_selection());
    }

    #[test]
    fn moving_the_cursor_after_starting_a_selection_extends_the_selected_text() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["hello world"]), vec![]);
        let mut session = CopySession::new(scrollback, 10);
        session.set_cursor(Cursor::new(0, 0));
        session.start_selection(SelectionMode::Char);

        let before = session.selected_text();
        session.set_cursor(Cursor::new(0, 4));
        let after = session.selected_text();

        assert_eq!(before, Some("h".to_string()));
        assert_eq!(after, Some("hello".to_string()));
    }

    #[test]
    fn toggle_selection_with_the_same_mode_clears_it() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["hello world"]), vec![]);
        let mut session = CopySession::new(scrollback, 10);
        session.toggle_selection(SelectionMode::Char);
        assert!(session.has_selection());

        session.toggle_selection(SelectionMode::Char);

        assert!(!session.has_selection());
        assert_eq!(session.selected_text(), None);
    }

    #[test]
    fn toggle_selection_with_a_different_mode_switches_and_keeps_the_anchor() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["hello world"]), vec![]);
        let mut session = CopySession::new(scrollback, 10);
        session.set_cursor(Cursor::new(0, 2));
        session.toggle_selection(SelectionMode::Char);
        let anchor = session.selection_anchor();

        session.toggle_selection(SelectionMode::Line);

        assert!(session.has_selection());
        assert_eq!(session.selection_mode(), SelectionMode::Line);
        assert_eq!(
            session.selection_anchor(),
            anchor,
            "switching mode must not move the anchor"
        );
    }

    #[test]
    fn set_selection_mode_preserves_the_anchor() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["hello world"]), vec![]);
        let mut session = CopySession::new(scrollback, 10);
        session.set_cursor(Cursor::new(0, 5));
        session.start_selection(SelectionMode::Char);
        let anchor = session.selection_anchor();

        session.set_selection_mode(SelectionMode::Block);

        assert_eq!(session.selection_mode(), SelectionMode::Block);
        assert_eq!(session.selection_anchor(), anchor);
    }

    #[test]
    fn set_selection_mode_with_no_active_selection_starts_one_at_the_cursor() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["hello world"]), vec![]);
        let mut session = CopySession::new(scrollback, 10);
        session.set_cursor(Cursor::new(0, 6));
        assert!(!session.has_selection());

        session.set_selection_mode(SelectionMode::Line);

        assert!(session.has_selection());
        assert_eq!(session.selection_anchor(), Some(Cursor::new(0, 6)));
        assert_eq!(session.selection_mode(), SelectionMode::Line);
    }

    #[test]
    fn clear_selection_makes_selected_text_none() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["hello world"]), vec![]);
        let mut session = CopySession::new(scrollback, 10);
        session.start_selection(SelectionMode::Char);
        assert!(session.selected_text().is_some());

        session.clear_selection();

        assert_eq!(session.selected_text(), None);
        assert!(!session.has_selection());
    }

    #[test]
    fn re_entering_copy_mode_drops_any_previous_selection() {
        let mut mode = CopyMode::default();
        mode.enter(
            Scrollback::from_parts(vec![], strings(&["first", "session"]), vec![]),
            10,
        );
        mode.session_mut()
            .unwrap()
            .start_selection(SelectionMode::Line);
        assert!(mode.session().unwrap().has_selection());

        mode.enter(
            Scrollback::from_parts(vec![], strings(&["second", "session"]), vec![]),
            10,
        );

        assert!(!mode.session().unwrap().has_selection());
        assert_eq!(mode.session().unwrap().selected_text(), None);
    }

    #[test]
    fn selected_text_is_none_with_no_active_selection() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["hello world"]), vec![]);
        let session = CopySession::new(scrollback, 10);

        assert_eq!(session.selected_text(), None);
    }

    #[test]
    fn selected_columns_for_row_returns_none_outside_the_selection() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["one", "two", "three"]), vec![]);
        let mut session = CopySession::new(scrollback, 10);
        session.set_cursor(Cursor::new(0, 0));
        session.start_selection(SelectionMode::Char);
        session.set_cursor(Cursor::new(1, 0));

        assert_eq!(session.selected_columns_for_row(2), None);
    }
}
