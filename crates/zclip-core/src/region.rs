//! tmux-style selection shapes over a captured [`Scrollback`].
//!
//! tmux's copy mode supports three distinct ways of interpreting the same
//! pair of grid coordinates: an arbitrary run of characters in reading
//! order, whole lines regardless of column, or a rectangular block of
//! columns. Each shape is genuinely useful for a different job — char-wise
//! for prose, line-wise for grabbing complete log lines without worrying
//! about where the cursor happens to sit within them, and block-wise for
//! pulling a single column out of tabular output (`ls -l`, `ps aux`, a
//! table) without dragging along the columns on either side of it.
//!
//! This module has exactly one job: given an anchor, a cursor, and a
//! [`SelectionMode`], turn that into text ([`extract_region`]) or into the
//! highlighted column range of a given row ([`selected_columns`], for a
//! renderer). Both functions must agree with each other — the tests below
//! assert that directly — because a renderer that highlights one range of
//! characters while copy actually captures a different range would be a
//! uniquely confusing bug for a user to notice.
//!
//! # Anchor and cursor are interchangeable
//!
//! Neither endpoint is privileged: the user may have dragged the selection
//! upward or leftward from where it started, and the result must be
//! identical either way. Both functions normalise their two endpoints
//! before doing anything else, exactly as [`crate::selection::extract_span`]
//! does for the plain char-wise case this module builds on.
//!
//! # The anchor lives in absolute row space
//!
//! Like the cursor itself (see [`crate::scrollback`]'s module docs), the
//! anchor is stored as a position in the scrollback's absolute row space,
//! not relative to whatever is currently scrolled into view. That is what
//! lets a selection stay correct as the viewport scrolls out from under it.

use crate::scrollback::Scrollback;
use crate::selection::{extract_span, Cursor};

/// Which of tmux's three selection shapes a copy-mode selection currently
/// uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    /// An arbitrary run of text in reading order: from the anchor, through
    /// the end of its line, through every line strictly between the two
    /// endpoints, up to the cursor's position on its own line. This is the
    /// shape prose and free-form text want.
    Char,
    /// Whole lines, regardless of which column the anchor or cursor sit on.
    /// Useful for grabbing complete log lines without needing to first
    /// land the cursor at column 0.
    Line,
    /// A rectangle: the same column range repeated on every row between the
    /// anchor and cursor. This is what makes columnar output (tables,
    /// `ls -l`) usable — it lets a user lift out one column without
    /// dragging along its neighbours.
    Block,
}

/// Normalises `anchor` and `cursor` into `(start, end)` such that `start`
/// never comes after `end` in reading order.
///
/// [`Cursor`]'s derived [`Ord`] compares `row` first, then `col`, so a
/// single comparison handles both "the cursor is on an earlier row" and
/// "same row, earlier column" at once. This is the same trick
/// [`extract_span`] uses internally; it is repeated here (rather than
/// relied upon indirectly) because [`selected_columns`] also needs a
/// normalised pair and does not call `extract_span` at all.
fn normalise(anchor: Cursor, cursor: Cursor) -> (Cursor, Cursor) {
    if cursor < anchor {
        (cursor, anchor)
    } else {
        (anchor, cursor)
    }
}

/// The inclusive `(min, max)` column range spanned by `anchor` and `cursor`,
/// compared independently of their rows.
///
/// Block selections care about columns alone, not reading order: a
/// selection dragged down-and-left has its anchor on an earlier row but a
/// *larger* column than the cursor, and the rectangle must still come out
/// with its columns the right way round.
fn column_bounds(anchor: Cursor, cursor: Cursor) -> (usize, usize) {
    (anchor.col.min(cursor.col), anchor.col.max(cursor.col))
}

/// The characters of `scrollback`'s line at `row`, from character offset
/// `min_col` through `max_col` inclusive.
///
/// A row with fewer than `min_col` characters contributes an **empty
/// string**, not a skipped row: [`extract_region`]'s block case relies on
/// this to preserve the rectangle's row count even when some rows are too
/// short to reach the selected columns at all. A row with at least
/// `min_col` but fewer than `max_col + 1` characters contributes whatever
/// it has from `min_col` onward.
fn block_slice(scrollback: &Scrollback, row: usize, min_col: usize, max_col: usize) -> String {
    let chars: Vec<char> = scrollback.line(row).unwrap_or("").chars().collect();
    if min_col >= chars.len() {
        return String::new();
    }
    let end = max_col.min(chars.len() - 1);
    chars[min_col..=end].iter().collect()
}

/// Extracts the text a selection from `anchor` to `cursor` covers,
/// interpreted according to `mode`.
///
/// # Semantics
///
/// - **Direction-independent.** `anchor` and `cursor` are normalised first
///   (see [`normalise`]), so dragging a selection upward or leftward
///   produces exactly the same text as the equivalent forward drag.
/// - **Never panics.** An empty `scrollback` returns an empty string
///   regardless of `mode`; out-of-range rows and columns are clamped rather
///   than indexed directly.
/// - [`SelectionMode::Char`] delegates to [`extract_span`], which is
///   end-inclusive and UTF-8 safe.
/// - [`SelectionMode::Line`] takes every line from the lower row through
///   the higher row inclusive, **verbatim** — columns play no part at all —
///   joined with `\n`.
/// - [`SelectionMode::Block`] takes, from every row in that same range, the
///   characters at the lower column through the higher column inclusive
///   (independently of row), joined with `\n`. See [`block_slice`] for how
///   short rows are handled.
///
/// [`selected_columns`] must agree with this function about which
/// characters a selection covers; see this module's tests for both.
#[must_use]
pub fn extract_region(
    scrollback: &Scrollback,
    anchor: Cursor,
    cursor: Cursor,
    mode: SelectionMode,
) -> String {
    if scrollback.is_empty() {
        return String::new();
    }

    let (start, end) = normalise(anchor, cursor);
    let min_row = scrollback.clamp_row(start.row);
    let max_row = scrollback.clamp_row(end.row);

    match mode {
        SelectionMode::Char => {
            let lines: Vec<&str> = scrollback.lines().iter().map(String::as_str).collect();
            extract_span(&lines, start, end)
        }
        SelectionMode::Line => (min_row..=max_row)
            .filter_map(|row| scrollback.line(row))
            .collect::<Vec<_>>()
            .join("\n"),
        SelectionMode::Block => {
            let (min_col, max_col) = column_bounds(anchor, cursor);
            (min_row..=max_row)
                .map(|row| block_slice(scrollback, row, min_col, max_col))
                .collect::<Vec<_>>()
                .join("\n")
        }
    }
}

/// The inclusive `(start_col, end_col)` range of `row` that a selection
/// from `anchor` to `cursor` covers, or `None` if `row` falls outside the
/// selection entirely.
///
/// Intended for a renderer that needs to highlight exactly the cells
/// [`extract_region`] would extract, without re-extracting the text itself
/// just to measure it. `line_width` is the character width of `row` as the
/// caller sees it (typically [`Scrollback::line_width`]) — this function
/// does not look `row` up in a scrollback itself, since a renderer already
/// has the width in hand while walking visible rows.
///
/// # Semantics
///
/// - `anchor` and `cursor` are normalised first, same as
///   [`extract_region`].
/// - A `row` outside the normalised `[min_row, max_row]` range yields
///   `None`.
/// - [`SelectionMode::Line`] selects the whole row: `Some((0, line_width -
///   1))`, or `Some((0, 0))` for an empty line (`line_width == 0`) so an
///   empty line can still be shown as selected rather than disappearing
///   from the highlight entirely.
/// - [`SelectionMode::Block`] selects the same `(min_col, max_col)` on
///   every row in range, regardless of that row's actual width — the
///   caller is expected to clamp against its own display width.
/// - [`SelectionMode::Char`] depends on where `row` falls within a
///   multi-row selection:
///   - a single-row selection selects `(start.col, end.col)`;
///   - the first row of a multi-row selection selects from `start.col` to
///     the end of the line;
///   - the last row selects from the start of the line to `end.col`;
///   - every row strictly between them selects the whole line.
#[must_use]
pub fn selected_columns(
    row: usize,
    anchor: Cursor,
    cursor: Cursor,
    mode: SelectionMode,
    line_width: usize,
) -> Option<(usize, usize)> {
    let (start, end) = normalise(anchor, cursor);
    if row < start.row || row > end.row {
        return None;
    }

    match mode {
        SelectionMode::Line => {
            if line_width == 0 {
                Some((0, 0))
            } else {
                Some((0, line_width - 1))
            }
        }
        SelectionMode::Block => {
            let (min_col, max_col) = column_bounds(anchor, cursor);
            Some((min_col, max_col))
        }
        SelectionMode::Char => {
            if start.row == end.row {
                Some((start.col, end.col))
            } else if row == start.row {
                Some((start.col, line_width.saturating_sub(1)))
            } else if row == end.row {
                Some((0, end.col))
            } else {
                Some((0, line_width.saturating_sub(1)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    /// A scrollback with two lines of history above a three-line viewport,
    /// so tests can select across that boundary. Row layout:
    /// `0: "history 0"`, `1: "history 1"`, `2: "view 0"`, `3: "view 1"`,
    /// `4: "view 2"`.
    fn sample_with_history() -> Scrollback {
        Scrollback::from_parts(
            strings(&["history 0", "history 1"]),
            strings(&["view 0", "view 1", "view 2"]),
            vec![],
        )
    }

    #[test]
    fn char_single_row_selection_extracts_the_substring() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["hello world"]), vec![]);
        let result = extract_region(
            &scrollback,
            Cursor::new(0, 0),
            Cursor::new(0, 4),
            SelectionMode::Char,
        );
        assert_eq!(result, "hello");
    }

    #[test]
    fn line_single_row_selection_returns_the_whole_line_ignoring_columns() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["hello world"]), vec![]);
        let result = extract_region(
            &scrollback,
            Cursor::new(0, 3),
            Cursor::new(0, 3),
            SelectionMode::Line,
        );
        assert_eq!(result, "hello world");
    }

    #[test]
    fn block_single_row_selection_extracts_the_column_range() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["hello world"]), vec![]);
        let result = extract_region(
            &scrollback,
            Cursor::new(0, 0),
            Cursor::new(0, 4),
            SelectionMode::Block,
        );
        assert_eq!(result, "hello");
    }

    #[test]
    fn char_multi_row_selection_produces_the_exact_expected_string() {
        let scrollback = Scrollback::from_parts(
            vec![],
            strings(&["first line", "middle", "last line"]),
            vec![],
        );
        let result = extract_region(
            &scrollback,
            Cursor::new(0, 6),
            Cursor::new(2, 3),
            SelectionMode::Char,
        );
        assert_eq!(result, "line\nmiddle\nlast");
    }

    #[test]
    fn line_multi_row_selection_joins_whole_lines_verbatim() {
        let scrollback = Scrollback::from_parts(
            vec![],
            strings(&["  padded start", "middle", "end   "]),
            vec![],
        );
        let result = extract_region(
            &scrollback,
            Cursor::new(0, 10),
            Cursor::new(2, 0),
            SelectionMode::Line,
        );
        assert_eq!(
            result, "  padded start\nmiddle\nend   ",
            "no trimming, columns are irrelevant"
        );
    }

    #[test]
    fn block_multi_row_selection_forms_a_rectangle() {
        let scrollback =
            Scrollback::from_parts(vec![], strings(&["abcdef", "ghijkl", "mnopqr"]), vec![]);
        let result = extract_region(
            &scrollback,
            Cursor::new(0, 1),
            Cursor::new(2, 3),
            SelectionMode::Block,
        );
        assert_eq!(result, "bcd\nhij\nnop");
    }

    #[test]
    fn reversed_anchor_and_cursor_produce_identical_output_for_char_mode() {
        let scrollback =
            Scrollback::from_parts(vec![], strings(&["first line", "last line"]), vec![]);
        let forward = extract_region(
            &scrollback,
            Cursor::new(0, 2),
            Cursor::new(1, 4),
            SelectionMode::Char,
        );
        let backward = extract_region(
            &scrollback,
            Cursor::new(1, 4),
            Cursor::new(0, 2),
            SelectionMode::Char,
        );
        assert_eq!(forward, backward);
    }

    #[test]
    fn reversed_anchor_and_cursor_produce_identical_output_for_line_mode() {
        let scrollback =
            Scrollback::from_parts(vec![], strings(&["first line", "last line"]), vec![]);
        let forward = extract_region(
            &scrollback,
            Cursor::new(0, 2),
            Cursor::new(1, 4),
            SelectionMode::Line,
        );
        let backward = extract_region(
            &scrollback,
            Cursor::new(1, 4),
            Cursor::new(0, 2),
            SelectionMode::Line,
        );
        assert_eq!(forward, backward);
    }

    #[test]
    fn reversed_anchor_and_cursor_produce_identical_output_for_block_mode() {
        let scrollback =
            Scrollback::from_parts(vec![], strings(&["abcdef", "ghijkl", "mnopqr"]), vec![]);
        let forward = extract_region(
            &scrollback,
            Cursor::new(0, 1),
            Cursor::new(2, 3),
            SelectionMode::Block,
        );
        let backward = extract_region(
            &scrollback,
            Cursor::new(2, 3),
            Cursor::new(0, 1),
            SelectionMode::Block,
        );
        assert_eq!(forward, backward);
    }

    #[test]
    fn block_selection_with_a_row_shorter_than_min_col_yields_an_empty_line_but_keeps_the_row_count(
    ) {
        let scrollback = Scrollback::from_parts(
            vec![],
            strings(&["a very long first line", "hi", "z"]),
            vec![],
        );
        let result = extract_region(
            &scrollback,
            Cursor::new(0, 5),
            Cursor::new(2, 8),
            SelectionMode::Block,
        );
        let rows: Vec<&str> = result.split('\n').collect();
        assert_eq!(rows.len(), 3, "the rectangle keeps all three rows");
        assert_eq!(rows[1], "", "row too short for min_col contributes nothing");
        assert_eq!(rows[2], "", "row 'z' is also shorter than min_col");
    }

    #[test]
    fn block_dragged_down_left_still_forms_a_proper_rectangle() {
        let scrollback =
            Scrollback::from_parts(vec![], strings(&["abcdef", "ghijkl", "mnopqr"]), vec![]);
        // Anchor at the top-right of the intended rectangle, cursor at the
        // bottom-left: min_row/max_row come from the rows, min_col/max_col
        // from the columns, independently of which endpoint is "the anchor".
        let anchor = Cursor::new(0, 3);
        let cursor = Cursor::new(2, 1);
        let result = extract_region(&scrollback, anchor, cursor, SelectionMode::Block);
        assert_eq!(result, "bcd\nhij\nnop");
    }

    #[test]
    fn selection_spans_the_viewport_history_boundary_in_char_mode() {
        let scrollback = sample_with_history();
        let result = extract_region(
            &scrollback,
            Cursor::new(1, 4),
            Cursor::new(2, 3),
            SelectionMode::Char,
        );
        assert_eq!(result, "ory 1\nview");
    }

    #[test]
    fn selection_spans_the_viewport_history_boundary_in_line_mode() {
        let scrollback = sample_with_history();
        let result = extract_region(
            &scrollback,
            Cursor::new(1, 0),
            Cursor::new(2, 0),
            SelectionMode::Line,
        );
        assert_eq!(result, "history 1\nview 0");
    }

    #[test]
    fn selection_spans_the_viewport_history_boundary_in_block_mode() {
        let scrollback = sample_with_history();
        let result = extract_region(
            &scrollback,
            Cursor::new(1, 0),
            Cursor::new(2, 3),
            SelectionMode::Block,
        );
        assert_eq!(result, "hist\nview");
    }

    #[test]
    fn block_selection_over_a_cjk_line_uses_character_columns_not_bytes() {
        let scrollback = Scrollback::from_parts(vec![], strings(&["日本語のテキスト"]), vec![]);
        let result = extract_region(
            &scrollback,
            Cursor::new(0, 0),
            Cursor::new(0, 2),
            SelectionMode::Block,
        );
        assert_eq!(result, "日本語");
    }

    #[test]
    fn empty_scrollback_returns_empty_string_in_char_mode() {
        let scrollback = Scrollback::empty();
        let result = extract_region(
            &scrollback,
            Cursor::new(0, 0),
            Cursor::new(5, 5),
            SelectionMode::Char,
        );
        assert_eq!(result, "");
    }

    #[test]
    fn empty_scrollback_returns_empty_string_in_line_mode() {
        let scrollback = Scrollback::empty();
        let result = extract_region(
            &scrollback,
            Cursor::new(0, 0),
            Cursor::new(5, 5),
            SelectionMode::Line,
        );
        assert_eq!(result, "");
    }

    #[test]
    fn empty_scrollback_returns_empty_string_in_block_mode() {
        let scrollback = Scrollback::empty();
        let result = extract_region(
            &scrollback,
            Cursor::new(0, 0),
            Cursor::new(5, 5),
            SelectionMode::Block,
        );
        assert_eq!(result, "");
    }

    #[test]
    fn selected_columns_returns_none_outside_the_selected_rows() {
        let anchor = Cursor::new(1, 0);
        let cursor = Cursor::new(3, 0);
        assert_eq!(
            selected_columns(0, anchor, cursor, SelectionMode::Char, 10),
            None
        );
        assert_eq!(
            selected_columns(4, anchor, cursor, SelectionMode::Char, 10),
            None
        );
    }

    #[test]
    fn selected_columns_char_mode_first_middle_and_last_row() {
        let anchor = Cursor::new(1, 4);
        let cursor = Cursor::new(3, 2);

        assert_eq!(
            selected_columns(1, anchor, cursor, SelectionMode::Char, 10),
            Some((4, 9)),
            "first row: from start.col to the end of the line"
        );
        assert_eq!(
            selected_columns(2, anchor, cursor, SelectionMode::Char, 10),
            Some((0, 9)),
            "middle row: the whole line"
        );
        assert_eq!(
            selected_columns(3, anchor, cursor, SelectionMode::Char, 10),
            Some((0, 2)),
            "last row: from the start of the line to end.col"
        );
    }

    #[test]
    fn selected_columns_agrees_with_extract_region_for_a_multi_row_char_selection() {
        let scrollback = Scrollback::from_parts(
            vec![],
            strings(&["first line", "middle", "last line"]),
            vec![],
        );
        let anchor = Cursor::new(0, 6);
        let cursor = Cursor::new(2, 3);

        let extracted = extract_region(&scrollback, anchor, cursor, SelectionMode::Char);

        let mut rebuilt = Vec::new();
        for row in 0..scrollback.len() {
            let width = scrollback.line_width(row);
            if let Some((start_col, end_col)) =
                selected_columns(row, anchor, cursor, SelectionMode::Char, width)
            {
                let chars: Vec<char> = scrollback.line(row).unwrap().chars().collect();
                let end = end_col.min(chars.len().saturating_sub(1));
                let slice: String = if chars.is_empty() {
                    String::new()
                } else {
                    chars[start_col.min(chars.len() - 1)..=end].iter().collect()
                };
                rebuilt.push(slice);
            }
        }

        assert_eq!(rebuilt.join("\n"), extracted);
    }
}
