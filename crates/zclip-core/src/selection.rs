//! Pure text-selection logic, independent of any terminal host.
//!
//! Terminal multiplexers (tmux, and Zellij itself) report selections as a
//! pair of grid coordinates and hand the host the job of turning that into
//! text. That extraction has a handful of easy-to-get-wrong subtleties:
//! reversed drag direction, end-inclusivity, out-of-range coordinates from a
//! stale or resized pane, and UTF-8 character vs. byte offsets. Modelling it
//! here, with no dependency on `zellij_tile` or any host type, means all of
//! that logic can be exercised with plain unit tests instead of requiring a
//! running Zellij session.
//!
//! This module also handles a second, related problem: terminal grids pad
//! every row out to the pane's full width with spaces, so raw selection
//! capture tends to produce lines with long, meaningless trailing
//! whitespace. [`trim_trailing_blanks`] cleans that up before text reaches a
//! [`crate::buffer::PasteBuffer`].

/// A zero-based position within a grid of text lines.
///
/// `row` indexes into a slice of lines; `col` is a **character** offset
/// within that line, not a byte offset. Terminal coordinates and Rust
/// `String` indexing both invite confusing the two, but a byte offset is
/// unsafe here: it can land in the middle of a multi-byte UTF-8 codepoint
/// and panic on slicing. Every function in this module treats `col` as a
/// `chars()` index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cursor {
    /// The zero-based line index.
    pub row: usize,
    /// The zero-based character offset within the line at `row`.
    pub col: usize,
}

impl Cursor {
    /// Creates a cursor at the given row and column.
    ///
    /// The derived [`Ord`] compares `row` first, then `col`, which is
    /// exactly row-major reading order: it lets callers write
    /// `start.min(end)` / `if end < start` to normalise a selection's
    /// direction without any extra logic.
    #[must_use]
    pub fn new(row: usize, col: usize) -> Self {
        Self { row, col }
    }
}

/// Extracts the text a terminal selection from `start` to `end` covers.
///
/// # Semantics
///
/// - **Direction-independent.** If `end` comes before `start` (the user
///   dragged the selection backward or upward), the two are swapped first,
///   so a backward drag produces exactly the same text as the equivalent
///   forward drag.
/// - **The end coordinate is inclusive.** Every terminal emulator (and
///   tmux) highlights the cell the selection ends on as part of the
///   selection, so that character is included in the output. This is the
///   opposite of the usual programming convention of an exclusive end
///   bound, chosen deliberately to match what users see highlighted on
///   screen.
/// - **Single-row selections** return the characters of that row from
///   `start.col` through `end.col` inclusive.
/// - **Multi-row selections** join, with `\n`, the tail of the start row
///   (from `start.col` to its end), the full text of every row strictly
///   between them, and the head of the end row (up to and including
///   `end.col`).
/// - **Never panics on out-of-range input.** Coordinates come from a
///   terminal host and may be stale (e.g. after a resize or scroll). If
///   `start.row` is beyond the end of `lines`, this returns an empty
///   string. `end.row` is clamped to the last valid row. Any `col` beyond
///   a line's character count is clamped to that line's length, so a
///   selection reaching past the end of a short line simply contributes
///   nothing further from that line rather than erroring.
/// - **UTF-8 safe.** All indexing goes through `chars()`, so this can never
///   slice through the middle of a multi-byte codepoint.
/// - Interior whitespace is preserved exactly; nothing is trimmed.
#[must_use]
pub fn extract_span(lines: &[&str], start: Cursor, end: Cursor) -> String {
    // Cursor's derived Ord is row-major, so this single comparison handles
    // both "end is on an earlier row" and "same row, end is an earlier
    // column" in one go.
    let (start, end) = if end < start {
        (end, start)
    } else {
        (start, end)
    };

    if start.row >= lines.len() {
        return String::new();
    }
    let end_row = end.row.min(lines.len() - 1);

    if start.row == end_row {
        return inclusive_char_range(lines[start.row], start.col, end.col);
    }

    let mut pieces: Vec<String> = Vec::with_capacity(end_row - start.row + 1);
    pieces.push(chars_from(lines[start.row], start.col));
    for line in &lines[start.row + 1..end_row] {
        pieces.push((*line).to_string());
    }
    pieces.push(inclusive_char_range(lines[end_row], 0, end.col));

    pieces.join("\n")
}

/// The characters of `line` from character offset `from` through the end,
/// clamping `from` to the line's length rather than panicking if it's out
/// of range. Used for the start row of a multi-row selection.
fn chars_from(line: &str, from: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    let from = from.min(chars.len());
    chars[from..].iter().collect()
}

/// The characters of `line` from character offset `from` through `to`
/// inclusive, clamping both to the line's available characters. Returns an
/// empty string for an empty line regardless of `from`/`to`, and an empty
/// string if `from` lands beyond the last character (nothing left to
/// include).
fn inclusive_char_range(line: &str, from: usize, to: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    let last = chars.len() - 1;
    let from = from.min(last);
    let to = to.min(last);
    if from > to {
        return String::new();
    }
    chars[from..=to].iter().collect()
}

/// Strips trailing spaces and tabs from every line of `text`.
///
/// Terminal panes pad each row out to the full pane width with spaces
/// (and sometimes tabs), so text captured directly from the grid comes
/// with long, meaningless whitespace tails. This removes exactly that,
/// while leaving everything a user would actually care about intact:
///
/// - Leading whitespace (indentation) is preserved verbatim.
/// - Interior content, including interior whitespace runs, is untouched.
/// - The number of lines is unchanged, and no trailing newline is added
///   or removed: splitting on `\n` and rejoining with `\n` round-trips the
///   line structure exactly.
/// - A line that is entirely whitespace becomes an empty string, i.e. a
///   genuinely blank line rather than one padded with spaces.
#[must_use]
pub fn trim_trailing_blanks(text: &str) -> String {
    text.split('\n')
        .map(|line| line.trim_end_matches([' ', '\t']))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_row_selection_is_end_inclusive() {
        let lines = ["hello"];
        let result = extract_span(&lines, Cursor::new(0, 0), Cursor::new(0, 2));
        assert_eq!(result, "hel", "col 2 must be included, not excluded");
    }

    #[test]
    fn single_row_start_equals_end_yields_one_character() {
        let lines = ["hello"];
        let result = extract_span(&lines, Cursor::new(0, 1), Cursor::new(0, 1));
        assert_eq!(result, "e");
    }

    #[test]
    fn multi_row_selection_spans_three_rows_exactly() {
        let lines = ["first line", "middle line", "last line"];
        let result = extract_span(&lines, Cursor::new(0, 6), Cursor::new(2, 3));
        assert_eq!(result, "line\nmiddle line\nlast");
    }

    #[test]
    fn two_row_selection_joins_start_tail_and_end_head() {
        let lines = ["abcdef", "ghijkl"];
        let result = extract_span(&lines, Cursor::new(0, 3), Cursor::new(1, 2));
        assert_eq!(result, "def\nghi");
    }

    #[test]
    fn reversed_coordinates_produce_identical_output_to_forward_selection() {
        let lines = ["first line", "middle line", "last line"];
        let forward = extract_span(&lines, Cursor::new(0, 6), Cursor::new(2, 3));
        let backward = extract_span(&lines, Cursor::new(2, 3), Cursor::new(0, 6));
        assert_eq!(forward, backward);
    }

    #[test]
    fn start_row_out_of_range_returns_empty_string() {
        let lines = ["only line"];
        let result = extract_span(&lines, Cursor::new(5, 0), Cursor::new(6, 0));
        assert_eq!(result, "");
    }

    #[test]
    fn end_row_past_the_end_clamps_to_the_last_line() {
        let lines = ["one", "two"];
        let result = extract_span(&lines, Cursor::new(0, 0), Cursor::new(99, 2));
        assert_eq!(result, "one\ntwo");
    }

    #[test]
    fn col_past_end_of_line_clamps_including_a_short_middle_line() {
        let lines = ["a long first line", "hi", "another long last line"];
        let result = extract_span(&lines, Cursor::new(0, 100), Cursor::new(2, 100));
        assert_eq!(
            result, "\nhi\nanother long last line",
            "start col beyond the first line's length contributes nothing from it; \
             the short middle line is untouched by any clamping; the end col beyond \
             the last line's length clamps to its final character"
        );
    }

    #[test]
    fn empty_lines_slice_returns_empty_string() {
        let lines: [&str; 0] = [];
        let result = extract_span(&lines, Cursor::new(0, 0), Cursor::new(0, 0));
        assert_eq!(result, "");
    }

    #[test]
    fn selection_over_empty_string_lines() {
        let lines = ["", "", ""];
        let result = extract_span(&lines, Cursor::new(0, 0), Cursor::new(2, 0));
        assert_eq!(result, "\n\n");
    }

    #[test]
    fn multibyte_utf8_selection_returns_correct_characters() {
        let lines = ["日本語のテキスト"];
        let result = extract_span(&lines, Cursor::new(0, 0), Cursor::new(0, 2));
        assert_eq!(result, "日本語");
    }

    #[test]
    fn mixed_ascii_and_multibyte_on_the_same_line() {
        let lines = ["ab日本語cd"];
        let result = extract_span(&lines, Cursor::new(0, 1), Cursor::new(0, 5));
        assert_eq!(result, "b日本語c");
    }

    #[test]
    fn multibyte_line_as_a_middle_row_of_a_multi_row_selection() {
        let lines = ["start", "日本語のテキスト", "end line"];
        let result = extract_span(&lines, Cursor::new(0, 2), Cursor::new(2, 2));
        assert_eq!(result, "art\n日本語のテキスト\nend");
    }

    #[test]
    fn interior_whitespace_is_preserved_verbatim() {
        let lines = ["a  b   c"];
        let result = extract_span(&lines, Cursor::new(0, 0), Cursor::new(0, 7));
        assert_eq!(result, "a  b   c");
    }

    #[test]
    fn trim_trailing_blanks_removes_trailing_spaces() {
        let result = trim_trailing_blanks("hello   \nworld");
        assert_eq!(result, "hello\nworld");
    }

    #[test]
    fn trim_trailing_blanks_preserves_leading_whitespace() {
        let result = trim_trailing_blanks("    indented   ");
        assert_eq!(result, "    indented");
    }

    #[test]
    fn trim_trailing_blanks_turns_whitespace_only_line_into_empty_string() {
        let result = trim_trailing_blanks("first\n     \nlast");
        assert_eq!(result, "first\n\nlast");
    }

    #[test]
    fn trim_trailing_blanks_leaves_interior_spaces_untouched() {
        let result = trim_trailing_blanks("a  b  c   ");
        assert_eq!(result, "a  b  c");
    }

    #[test]
    fn trim_trailing_blanks_preserves_line_count() {
        let input = "one  \ntwo  \nthree  ";
        let result = trim_trailing_blanks(input);
        assert_eq!(result.split('\n').count(), input.split('\n').count());
    }

    #[test]
    fn trim_trailing_blanks_without_trailing_newline_adds_none() {
        let result = trim_trailing_blanks("no newline   ");
        assert_eq!(result, "no newline");
        assert!(!result.ends_with('\n'));
    }

    #[test]
    fn trim_trailing_blanks_with_trailing_newline_keeps_it() {
        let result = trim_trailing_blanks("line one   \n");
        assert_eq!(result, "line one\n");
    }

    #[test]
    fn trim_trailing_blanks_treats_tabs_as_trailing_blanks() {
        let result = trim_trailing_blanks("hello\t\t\nworld");
        assert_eq!(result, "hello\nworld");
    }
}
