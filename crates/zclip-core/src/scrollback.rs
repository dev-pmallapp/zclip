//! A captured snapshot of a terminal pane's text.
//!
//! Zellij (like tmux) reports a pane's scrollback in three pieces: the lines
//! scrolled off above the current viewport, the viewport itself, and any
//! lines below it (a pane can be scrolled up, so "below" is a real,
//! non-empty case, not just padding). [`Scrollback`] flattens those three
//! pieces into one contiguous sequence with **stable absolute row indices**:
//! row `0` is always the oldest captured line, no matter where the viewport
//! currently sits within it.
//!
//! # Why absolute indices matter
//!
//! The viewport is a moving window over captured history: as the user
//! scrolls, the same on-screen row corresponds to a different line. A
//! cursor or highlight that addressed rows *relative to the viewport* would
//! therefore silently point at something else the moment the viewport
//! moved — exactly the kind of bug that is invisible in a quick manual test
//! and infuriating to track down later. By contrast, an absolute row index
//! keeps meaning "this specific captured line" regardless of how the
//! viewport is scrolled around it, so copy-mode state (cursor position,
//! selection endpoints) can be stored once and stay correct.
//!
//! # This is a snapshot
//!
//! A [`Scrollback`] is captured at a single point in time. A pane that keeps
//! producing output after capture will drift from what is stored here; nothing
//! in this module re-synchronises automatically. Callers that care about
//! staying current (e.g. re-entering copy mode) are expected to capture a
//! fresh [`Scrollback`] rather than expect this one to update itself.

use crate::selection::Cursor;

/// A flattened, absolutely-indexed snapshot of a pane's captured text.
///
/// See the module docs for why row indices are absolute rather than
/// viewport-relative, and for why this is a point-in-time snapshot rather
/// than a live view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scrollback {
    lines: Vec<String>,
    viewport_start: usize,
    viewport_len: usize,
}

impl Default for Scrollback {
    /// An empty snapshot, equivalent to [`Scrollback::empty`].
    fn default() -> Self {
        Self::empty()
    }
}

impl Scrollback {
    /// Builds a snapshot from the three pieces a host typically reports.
    ///
    /// `above` becomes rows `[0, above.len())`, `viewport` becomes the rows
    /// immediately after it, and `below` follows that. This is the one place
    /// the absolute/relative translation happens, so every other method in
    /// this module can work purely in absolute rows.
    #[must_use]
    pub fn from_parts(above: Vec<String>, viewport: Vec<String>, below: Vec<String>) -> Self {
        let viewport_start = above.len();
        let viewport_len = viewport.len();
        let mut lines = above;
        lines.extend(viewport);
        lines.extend(below);
        Self {
            lines,
            viewport_start,
            viewport_len,
        }
    }

    /// An empty snapshot: no captured lines at all.
    ///
    /// Useful as a starting point before any pane has been captured, and in
    /// tests that only care about behaviour that does not depend on content.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            lines: Vec::new(),
            viewport_start: 0,
            viewport_len: 0,
        }
    }

    /// The total number of captured lines, across all three original parts.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether this snapshot has no captured lines at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// The line at absolute row `row`, or `None` if `row` is out of range.
    #[must_use]
    pub fn line(&self, row: usize) -> Option<&str> {
        self.lines.get(row).map(String::as_str)
    }

    /// All captured lines, in absolute row order.
    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// The absolute row index of the first line that was in the viewport at
    /// capture time.
    #[must_use]
    pub fn viewport_start(&self) -> usize {
        self.viewport_start
    }

    /// The absolute row index one past the last viewport line, i.e. the
    /// exclusive end of the `[viewport_start, viewport_end)` range.
    #[must_use]
    pub fn viewport_end(&self) -> usize {
        self.viewport_start + self.viewport_len
    }

    /// The absolute index of the last captured line, or `0` if there are
    /// none.
    ///
    /// This deliberately does not distinguish "the only line is row 0" from
    /// "there are no lines" — callers that need to tell those apart should
    /// check [`Self::is_empty`] first.
    #[must_use]
    pub fn last_row(&self) -> usize {
        self.len().saturating_sub(1)
    }

    /// Clamps `row` into the valid range for this snapshot.
    ///
    /// An empty snapshot clamps everything to `0`, matching [`Self::last_row`].
    #[must_use]
    pub fn clamp_row(&self, row: usize) -> usize {
        row.min(self.last_row())
    }

    /// The number of **characters** (not bytes) in the line at `row`.
    ///
    /// Terminal lines are captured as UTF-8 text, and a multi-byte
    /// character (e.g. from CJK text) must count as one column, not as
    /// however many bytes it happens to encode to. A row past the end of
    /// the snapshot has a width of `0`.
    #[must_use]
    pub fn line_width(&self, row: usize) -> usize {
        self.line(row).map_or(0, |line| line.chars().count())
    }

    /// Clamps a cursor so it addresses a real position within this snapshot:
    /// the row is clamped via [`Self::clamp_row`], and the column is then
    /// clamped to that (possibly different) row's character width.
    ///
    /// This is the funnel every cursor movement should go through before
    /// being stored, so that a cursor computed against stale coordinates
    /// (e.g. before a resize or re-capture) can never point past the end of
    /// a line or past the end of the snapshot.
    #[must_use]
    pub fn clamp_cursor(&self, cursor: Cursor) -> Cursor {
        let row = self.clamp_row(cursor.row);
        let col = cursor.col.min(self.line_width(row));
        Cursor::new(row, col)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn empty_scrollback_has_no_lines_and_an_empty_viewport() {
        let scrollback = Scrollback::empty();

        assert_eq!(scrollback.len(), 0);
        assert!(scrollback.is_empty());
        assert_eq!(scrollback.viewport_start(), 0);
        assert_eq!(scrollback.viewport_end(), 0);
    }

    #[test]
    fn default_matches_empty() {
        assert_eq!(Scrollback::default(), Scrollback::empty());
    }

    #[test]
    fn viewport_only_has_no_history_above_or_below() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["a", "b"]), vec![]);

        assert_eq!(scrollback.len(), 2);
        assert_eq!(scrollback.viewport_start(), 0);
        assert_eq!(scrollback.viewport_end(), 2);
    }

    #[test]
    fn all_three_parts_are_flattened_with_correct_viewport_bounds() {
        let above = strings(&["above 1", "above 2"]);
        let viewport = strings(&["view 1", "view 2", "view 3"]);
        let below = strings(&["below 1"]);
        let scrollback = Scrollback::from_parts(above, viewport, below);

        assert_eq!(scrollback.len(), 6);
        assert_eq!(scrollback.viewport_start(), 2, "two lines of history above");
        assert_eq!(
            scrollback.viewport_end(),
            5,
            "viewport is 3 lines starting at row 2"
        );
        assert_eq!(scrollback.line(0), Some("above 1"));
        assert_eq!(scrollback.line(2), Some("view 1"));
        assert_eq!(scrollback.line(5), Some("below 1"));
    }

    #[test]
    fn line_returns_none_out_of_range_and_some_in_range() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["only line"]), vec![]);

        assert_eq!(scrollback.line(0), Some("only line"));
        assert_eq!(scrollback.line(1), None);
        assert_eq!(scrollback.line(100), None);
    }

    #[test]
    fn clamp_row_on_empty_scrollback_is_always_zero() {
        let scrollback = Scrollback::empty();

        assert_eq!(scrollback.clamp_row(0), 0);
        assert_eq!(scrollback.clamp_row(5), 0);
    }

    #[test]
    fn clamp_row_on_populated_scrollback_clamps_to_the_last_row() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["a", "b", "c"]), vec![]);

        assert_eq!(scrollback.clamp_row(0), 0);
        assert_eq!(scrollback.clamp_row(2), 2);
        assert_eq!(scrollback.clamp_row(99), 2);
    }

    #[test]
    fn clamp_cursor_clamps_row_and_col_independently() {
        let scrollback =
            Scrollback::from_parts(vec![], strings(&["short", "a longer line"]), vec![]);

        let clamped = scrollback.clamp_cursor(Cursor::new(99, 99));
        assert_eq!(clamped.row, 1, "row clamps to the last row");
        assert_eq!(
            clamped.col, 13,
            "col clamps to the clamped row's own width, not the original row's"
        );
    }

    #[test]
    fn line_width_counts_multibyte_characters_not_bytes() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["日本語"]), vec![]);

        assert_eq!(
            scrollback.line_width(0),
            3,
            "three characters, even though the line is 9 bytes"
        );
    }

    #[test]
    fn clamp_cursor_col_clamps_correctly_on_a_multibyte_line() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["日本語"]), vec![]);

        let clamped = scrollback.clamp_cursor(Cursor::new(0, 100));
        assert_eq!(clamped.row, 0);
        assert_eq!(
            clamped.col, 3,
            "clamps to the character count, not byte count"
        );
    }
}
