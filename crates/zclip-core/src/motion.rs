//! Vi-style cursor motions over a captured [`Scrollback`].
//!
//! Vi (and vim, and every terminal pager that copies its keybindings) has a
//! small, well-known vocabulary of cursor motions: `h`/`j`/`k`/`l`, `w`/`b`/`e`,
//! `0`/`^`/`$`, `gg`/`G`, `Ctrl-u`/`Ctrl-d`/`Ctrl-b`/`Ctrl-f`. Users who reach
//! for zclip's copy mode expect that vocabulary to work exactly as muscle
//! memory says it should.
//!
//! This module models that vocabulary as data, not behaviour glued to a
//! keyboard: [`Motion`] is a plain enum naming *what* movement is requested,
//! and [`apply`] is the one function that turns a `Motion` plus the current
//! [`Cursor`] and [`Scrollback`] into a new `Cursor`. It deliberately knows
//! nothing about which physical key produced a given `Motion` — that mapping
//! (e.g. deciding that `Ctrl-d` means [`Motion::HalfPageDown`]) belongs to the
//! Zellij-facing plugin crate, which owns key types and host calls. Keeping
//! that translation out of here is what makes every motion in this module
//! testable with nothing but plain data.
//!
//! # The "desired column" simplification
//!
//! Real vi remembers a *desired column* that survives moving through short
//! lines: pressing `j` onto a short line clamps the visible column, but
//! pressing `j` again onto a longer line restores the original column as if
//! it had never been clamped. This module does not model that: every
//! vertical motion clamps the cursor's column to whatever line it lands on
//! and nothing else remembers the pre-clamp column. Moving down through a
//! short line and back up will **not** restore the original column. This is
//! a deliberate, documented gap rather than a bug — adding "desired column"
//! state would mean this module could no longer be a pure function of its
//! arguments, and no caller has needed it yet.
//!
//! # Word motions do not wrap lines
//!
//! Real vi's `w`/`b`/`e` motions cross line boundaries: `w` on the last word
//! of a line moves onto the next line, and `b` at the start of a line moves
//! onto the end of the previous one. This module's word motions operate on
//! the current line only, again deliberately: crossing lines correctly
//! requires deciding whether a line break itself counts as a word boundary,
//! which varies between vi implementations, and no caller of this module
//! has needed cross-line word motion yet.

use crate::scrollback::Scrollback;
use crate::selection::Cursor;

/// A single vi-style cursor movement, independent of whatever key produced
/// it.
///
/// The comment after each variant names vi's usual key for it, purely as a
/// reading aid; this module never touches a key type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    /// One character left, without wrapping to the previous line. (`h`)
    Left,
    /// One character right, without wrapping to the next line. (`l`)
    Right,
    /// One line up, preserving column where possible. (`k`)
    Up,
    /// One line down, preserving column where possible. (`j`)
    Down,
    /// The first column of the current line. (`0`)
    LineStart,
    /// The first non-blank character of the current line. (`^`)
    LineFirstNonBlank,
    /// One past the last character of the current line. (`$`)
    LineEnd,
    /// The start of the next word on the current line. (`w`)
    WordForward,
    /// The start of the current or previous word on the current line. (`b`)
    WordBackward,
    /// The end of the next word on the current line. (`e`)
    WordEnd,
    /// The very first line of the scrollback. (`gg`)
    Top,
    /// The very last line of the scrollback. (`G`)
    Bottom,
    /// Half a page up. (`Ctrl-u`)
    HalfPageUp,
    /// Half a page down. (`Ctrl-d`)
    HalfPageDown,
    /// A full page up. (`Ctrl-b`)
    PageUp,
    /// A full page down. (`Ctrl-f`)
    PageDown,
}

/// Applies `motion` to `cursor`, returning where it lands.
///
/// `height` (the pane's visible height in rows) is consulted only by the
/// paging motions ([`Motion::HalfPageUp`], [`Motion::HalfPageDown`],
/// [`Motion::PageUp`], [`Motion::PageDown`]). It is taken as a parameter
/// rather than stored anywhere because a pane can be resized between one
/// render and the next; passing it fresh on every call means this function
/// can never act on a stale height. A `height` of `0` is treated as `1`, so
/// a motion is always able to make progress rather than becoming a no-op.
///
/// This function never panics. On an empty scrollback every motion returns
/// `Cursor::new(0, 0)`. Otherwise, `cursor` is first clamped into the
/// scrollback (via [`Scrollback::clamp_cursor`]) before the motion is
/// computed, and the result is clamped again before being returned, so the
/// output always satisfies [`Scrollback::clamp_cursor`] regardless of
/// whether the input `cursor` was already valid.
#[must_use]
pub fn apply(motion: Motion, scrollback: &Scrollback, cursor: Cursor, height: usize) -> Cursor {
    if scrollback.is_empty() {
        return Cursor::new(0, 0);
    }

    let height = height.max(1);
    let cursor = scrollback.clamp_cursor(cursor);

    let result = match motion {
        Motion::Left => Cursor::new(cursor.row, cursor.col.saturating_sub(1)),
        Motion::Right => Cursor::new(cursor.row, cursor.col.saturating_add(1)),
        Motion::Up => Cursor::new(cursor.row.saturating_sub(1), cursor.col),
        Motion::Down => Cursor::new(cursor.row.saturating_add(1), cursor.col),
        Motion::LineStart => Cursor::new(cursor.row, 0),
        Motion::LineFirstNonBlank => {
            Cursor::new(cursor.row, first_non_blank_col(scrollback, cursor.row))
        }
        Motion::LineEnd => Cursor::new(cursor.row, scrollback.line_width(cursor.row)),
        Motion::WordForward => Cursor::new(cursor.row, word_forward_col(scrollback, cursor)),
        Motion::WordBackward => Cursor::new(cursor.row, word_backward_col(scrollback, cursor)),
        Motion::WordEnd => Cursor::new(cursor.row, word_end_col(scrollback, cursor)),
        Motion::Top => Cursor::new(0, 0),
        Motion::Bottom => Cursor::new(scrollback.last_row(), 0),
        Motion::HalfPageUp => {
            let step = (height / 2).max(1);
            Cursor::new(cursor.row.saturating_sub(step), cursor.col)
        }
        Motion::HalfPageDown => {
            let step = (height / 2).max(1);
            Cursor::new(cursor.row.saturating_add(step), cursor.col)
        }
        Motion::PageUp => Cursor::new(cursor.row.saturating_sub(height), cursor.col),
        Motion::PageDown => Cursor::new(cursor.row.saturating_add(height), cursor.col),
    };

    scrollback.clamp_cursor(result)
}

/// The column of the first non-whitespace character on `row`, or `0` if the
/// line is blank, empty, or out of range.
fn first_non_blank_col(scrollback: &Scrollback, row: usize) -> usize {
    scrollback.line(row).map_or(0, |line| {
        line.chars().position(|c| !c.is_whitespace()).unwrap_or(0)
    })
}

/// The characters of `row` as a `Vec<char>`, so word motions can index by
/// character rather than by byte. An out-of-range row yields an empty
/// vector.
fn line_chars(scrollback: &Scrollback, row: usize) -> Vec<char> {
    scrollback
        .line(row)
        .map(|line| line.chars().collect())
        .unwrap_or_default()
}

/// The three-way classification vi's word motions use to decide where one
/// "word" ends and the next begins: a run of word characters is one word, a
/// run of punctuation is a separate word, and whitespace separates both but
/// is never itself part of a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharClass {
    /// A space, tab, or other whitespace character.
    Whitespace,
    /// An alphanumeric character or underscore.
    Word,
    /// Any other non-whitespace character.
    Punctuation,
}

/// Classifies a single character for word-motion purposes.
fn classify(c: char) -> CharClass {
    if c.is_whitespace() {
        CharClass::Whitespace
    } else if c.is_alphanumeric() || c == '_' {
        CharClass::Word
    } else {
        CharClass::Punctuation
    }
}

/// The column `w` (word-forward) lands on: the start of the next word after
/// `cursor`, or the end of the line if no word remains.
fn word_forward_col(scrollback: &Scrollback, cursor: Cursor) -> usize {
    let chars = line_chars(scrollback, cursor.row);
    let len = chars.len();
    let mut i = cursor.col;
    if i >= len {
        return len;
    }

    let start_class = classify(chars[i]);
    if start_class != CharClass::Whitespace {
        while i < len && classify(chars[i]) == start_class {
            i += 1;
        }
    }
    while i < len && classify(chars[i]) == CharClass::Whitespace {
        i += 1;
    }
    i
}

/// The column `e` (word-end) lands on: the last character of the next word
/// end strictly after `cursor`, or the line's last character index if no
/// word end remains after it.
fn word_end_col(scrollback: &Scrollback, cursor: Cursor) -> usize {
    let chars = line_chars(scrollback, cursor.row);
    let len = chars.len();
    if len == 0 {
        return 0;
    }

    let mut i = cursor.col;
    if i < len {
        i += 1;
    }
    while i < len && classify(chars[i]) == CharClass::Whitespace {
        i += 1;
    }
    if i >= len {
        return len - 1;
    }

    let class = classify(chars[i]);
    while i + 1 < len && classify(chars[i + 1]) == class {
        i += 1;
    }
    i
}

/// The column `b` (word-backward) lands on: the start of the current word
/// if `cursor` sits past that word's first character, otherwise the start
/// of the previous word, or `0` if no earlier word exists on the line.
fn word_backward_col(scrollback: &Scrollback, cursor: Cursor) -> usize {
    let chars = line_chars(scrollback, cursor.row);
    if chars.is_empty() || cursor.col == 0 {
        return 0;
    }

    let mut i = cursor.col - 1;
    while i > 0 && classify(chars[i]) == CharClass::Whitespace {
        i -= 1;
    }
    if classify(chars[i]) == CharClass::Whitespace {
        return 0;
    }

    let class = classify(chars[i]);
    while i > 0 && classify(chars[i - 1]) == class {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    fn scrollback_of(lines: &[&str]) -> Scrollback {
        Scrollback::from_parts(vec![], strings(lines), vec![])
    }

    #[test]
    fn left_moves_one_column_left() {
        let scrollback = scrollback_of(&["hello"]);
        let result = apply(Motion::Left, &scrollback, Cursor::new(0, 3), 10);
        assert_eq!(result, Cursor::new(0, 2));
    }

    #[test]
    fn left_at_column_zero_stays_put_and_does_not_wrap_to_previous_line() {
        let scrollback = scrollback_of(&["one", "two"]);
        let result = apply(Motion::Left, &scrollback, Cursor::new(1, 0), 10);
        assert_eq!(result, Cursor::new(1, 0), "must not wrap to row 0");
    }

    #[test]
    fn right_moves_one_column_right() {
        let scrollback = scrollback_of(&["hello"]);
        let result = apply(Motion::Right, &scrollback, Cursor::new(0, 1), 10);
        assert_eq!(result, Cursor::new(0, 2));
    }

    #[test]
    fn right_at_end_of_line_stays_put_and_does_not_wrap_to_next_line() {
        let scrollback = scrollback_of(&["hi", "there"]);
        let result = apply(Motion::Right, &scrollback, Cursor::new(0, 2), 10);
        assert_eq!(result, Cursor::new(0, 2), "must not wrap to row 1");
    }

    #[test]
    fn up_moves_one_row_up_preserving_column() {
        let scrollback = scrollback_of(&["one", "two", "three"]);
        let result = apply(Motion::Up, &scrollback, Cursor::new(2, 1), 10);
        assert_eq!(result, Cursor::new(1, 1));
    }

    #[test]
    fn up_at_row_zero_stays_put() {
        let scrollback = scrollback_of(&["one", "two"]);
        let result = apply(Motion::Up, &scrollback, Cursor::new(0, 1), 10);
        assert_eq!(result, Cursor::new(0, 1));
    }

    #[test]
    fn down_moves_one_row_down_preserving_column() {
        let scrollback = scrollback_of(&["one", "two", "three"]);
        let result = apply(Motion::Down, &scrollback, Cursor::new(0, 2), 10);
        assert_eq!(result, Cursor::new(1, 2));
    }

    #[test]
    fn down_at_last_row_stays_put() {
        let scrollback = scrollback_of(&["one", "two"]);
        let result = apply(Motion::Down, &scrollback, Cursor::new(1, 1), 10);
        assert_eq!(result, Cursor::new(1, 1));
    }

    #[test]
    fn moving_onto_a_shorter_line_clamps_the_column() {
        let scrollback = scrollback_of(&["a long first line", "hi"]);
        let result = apply(Motion::Down, &scrollback, Cursor::new(0, 15), 10);
        assert_eq!(
            result,
            Cursor::new(1, 2),
            "clamped to the short line's width"
        );
    }

    #[test]
    fn moving_back_onto_the_longer_line_does_not_restore_the_original_column() {
        // Pins the documented "no desired column" simplification: down onto
        // a short line clamps col, and up again does *not* remember the
        // original column, unlike real vi.
        let scrollback = scrollback_of(&["a long first line", "hi"]);
        let after_down = apply(Motion::Down, &scrollback, Cursor::new(0, 15), 10);
        let after_up = apply(Motion::Up, &scrollback, after_down, 10);
        assert_eq!(
            after_up,
            Cursor::new(0, 2),
            "column stays clamped to 2, not restored to the original 15"
        );
    }

    #[test]
    fn line_start_moves_to_column_zero() {
        let scrollback = scrollback_of(&["hello"]);
        let result = apply(Motion::LineStart, &scrollback, Cursor::new(0, 4), 10);
        assert_eq!(result, Cursor::new(0, 0));
    }

    #[test]
    fn line_first_non_blank_skips_leading_whitespace() {
        let scrollback = scrollback_of(&["    indented"]);
        let result = apply(
            Motion::LineFirstNonBlank,
            &scrollback,
            Cursor::new(0, 0),
            10,
        );
        assert_eq!(result, Cursor::new(0, 4));
    }

    #[test]
    fn line_first_non_blank_on_all_whitespace_line_is_column_zero() {
        let scrollback = scrollback_of(&["      "]);
        let result = apply(
            Motion::LineFirstNonBlank,
            &scrollback,
            Cursor::new(0, 0),
            10,
        );
        assert_eq!(result, Cursor::new(0, 0));
    }

    #[test]
    fn line_first_non_blank_on_empty_line_is_column_zero() {
        let scrollback = scrollback_of(&[""]);
        let result = apply(
            Motion::LineFirstNonBlank,
            &scrollback,
            Cursor::new(0, 0),
            10,
        );
        assert_eq!(result, Cursor::new(0, 0));
    }

    #[test]
    fn line_end_moves_one_past_the_last_character() {
        let scrollback = scrollback_of(&["hello"]);
        let result = apply(Motion::LineEnd, &scrollback, Cursor::new(0, 0), 10);
        assert_eq!(result, Cursor::new(0, 5));
    }

    #[test]
    fn line_end_on_empty_line_is_zero() {
        let scrollback = scrollback_of(&[""]);
        let result = apply(Motion::LineEnd, &scrollback, Cursor::new(0, 0), 10);
        assert_eq!(result, Cursor::new(0, 0));
    }

    #[test]
    fn top_moves_to_the_first_row_and_column() {
        let scrollback = scrollback_of(&["one", "two", "three"]);
        let result = apply(Motion::Top, &scrollback, Cursor::new(2, 3), 10);
        assert_eq!(result, Cursor::new(0, 0));
    }

    #[test]
    fn bottom_moves_to_the_last_row_column_zero() {
        let scrollback = scrollback_of(&["one", "two", "three"]);
        let result = apply(Motion::Bottom, &scrollback, Cursor::new(0, 1), 10);
        assert_eq!(result, Cursor::new(2, 0));
    }

    #[test]
    fn half_page_down_advances_by_half_the_height() {
        let lines: Vec<&str> = vec!["l"; 20];
        let scrollback = scrollback_of(&lines);
        let result = apply(Motion::HalfPageDown, &scrollback, Cursor::new(0, 0), 10);
        assert_eq!(result, Cursor::new(5, 0));
    }

    #[test]
    fn half_page_down_with_height_one_still_advances_by_one() {
        let scrollback = scrollback_of(&["one", "two"]);
        let result = apply(Motion::HalfPageDown, &scrollback, Cursor::new(0, 0), 1);
        assert_eq!(result, Cursor::new(1, 0));
    }

    #[test]
    fn half_page_up_clamps_at_the_top() {
        let lines: Vec<&str> = vec!["l"; 20];
        let scrollback = scrollback_of(&lines);
        let result = apply(Motion::HalfPageUp, &scrollback, Cursor::new(2, 0), 10);
        assert_eq!(result, Cursor::new(0, 0));
    }

    #[test]
    fn page_down_clamps_at_the_bottom() {
        let scrollback = scrollback_of(&["one", "two", "three"]);
        let result = apply(Motion::PageDown, &scrollback, Cursor::new(0, 0), 100);
        assert_eq!(result, Cursor::new(2, 0));
    }

    #[test]
    fn page_up_clamps_at_the_top() {
        let scrollback = scrollback_of(&["one", "two", "three"]);
        let result = apply(Motion::PageUp, &scrollback, Cursor::new(1, 0), 100);
        assert_eq!(result, Cursor::new(0, 0));
    }

    #[test]
    fn height_zero_does_not_panic_and_behaves_like_height_one() {
        let scrollback = scrollback_of(&["one", "two", "three"]);
        let page_down = apply(Motion::PageDown, &scrollback, Cursor::new(0, 0), 0);
        assert_eq!(page_down, Cursor::new(1, 0));
        let half_down = apply(Motion::HalfPageDown, &scrollback, Cursor::new(0, 0), 0);
        assert_eq!(half_down, Cursor::new(1, 0));
    }

    #[test]
    fn word_forward_moves_across_simple_words() {
        let scrollback = scrollback_of(&["foo bar baz"]);
        let result = apply(Motion::WordForward, &scrollback, Cursor::new(0, 0), 10);
        assert_eq!(result, Cursor::new(0, 4), "start of \"bar\"");
    }

    #[test]
    fn word_forward_treats_punctuation_runs_as_separate_words() {
        let scrollback = scrollback_of(&["foo.bar(baz)"]);
        let result = apply(Motion::WordForward, &scrollback, Cursor::new(0, 0), 10);
        assert_eq!(result, Cursor::new(0, 3), "the '.' is its own word");
    }

    #[test]
    fn word_forward_skips_leading_and_trailing_whitespace() {
        let scrollback = scrollback_of(&["  foo   bar"]);
        let result = apply(Motion::WordForward, &scrollback, Cursor::new(0, 0), 10);
        assert_eq!(
            result,
            Cursor::new(0, 2),
            "lands on the first word, not col 0"
        );
    }

    #[test]
    fn word_forward_at_the_last_word_goes_to_end_of_line() {
        let scrollback = scrollback_of(&["foo bar"]);
        let result = apply(Motion::WordForward, &scrollback, Cursor::new(0, 4), 10);
        assert_eq!(result, Cursor::new(0, 7), "no next word, so end of line");
    }

    #[test]
    fn word_backward_moves_to_start_of_previous_word() {
        let scrollback = scrollback_of(&["foo bar baz"]);
        let result = apply(Motion::WordBackward, &scrollback, Cursor::new(0, 8), 10);
        assert_eq!(result, Cursor::new(0, 4), "start of \"bar\", not \"baz\"");
    }

    #[test]
    fn word_backward_inside_a_word_goes_to_that_words_own_start() {
        let scrollback = scrollback_of(&["foo bar baz"]);
        let result = apply(Motion::WordBackward, &scrollback, Cursor::new(0, 9), 10);
        assert_eq!(
            result,
            Cursor::new(0, 8),
            "inside \"baz\", past its first char"
        );
    }

    #[test]
    fn word_backward_at_column_zero_stays_put() {
        let scrollback = scrollback_of(&["foo bar"]);
        let result = apply(Motion::WordBackward, &scrollback, Cursor::new(0, 0), 10);
        assert_eq!(result, Cursor::new(0, 0));
    }

    #[test]
    fn word_end_from_first_character_of_a_word_goes_to_that_words_end() {
        let scrollback = scrollback_of(&["foo bar"]);
        let result = apply(Motion::WordEnd, &scrollback, Cursor::new(0, 0), 10);
        assert_eq!(result, Cursor::new(0, 2), "last char of \"foo\"");
    }

    #[test]
    fn word_end_already_at_a_words_end_advances_to_the_next_words_end() {
        let scrollback = scrollback_of(&["foo bar"]);
        let result = apply(Motion::WordEnd, &scrollback, Cursor::new(0, 2), 10);
        assert_eq!(result, Cursor::new(0, 6), "last char of \"bar\"");
    }

    #[test]
    fn word_end_with_no_word_remaining_goes_to_the_lines_last_character() {
        let scrollback = scrollback_of(&["foo bar"]);
        let result = apply(Motion::WordEnd, &scrollback, Cursor::new(0, 6), 10);
        assert_eq!(result, Cursor::new(0, 6), "already the last character");
    }

    #[test]
    fn word_motions_on_multibyte_utf8_line_use_character_indices() {
        let scrollback = scrollback_of(&["日本語 テキスト"]);

        let forward = apply(Motion::WordForward, &scrollback, Cursor::new(0, 0), 10);
        assert_eq!(
            forward,
            Cursor::new(0, 4),
            "index 4 is the 'テ' after the space, not a byte offset"
        );

        let end = apply(Motion::Right, &scrollback, Cursor::new(0, 0), 10);
        assert_eq!(end, Cursor::new(0, 1), "right moves by one character");

        let line_end = apply(Motion::LineEnd, &scrollback, Cursor::new(0, 0), 10);
        assert_eq!(
            line_end,
            Cursor::new(0, 8),
            "8 characters total, not the much larger byte length"
        );
    }

    #[test]
    fn every_motion_on_an_empty_scrollback_returns_origin_without_panicking() {
        let scrollback = Scrollback::empty();
        let motions = [
            Motion::Left,
            Motion::Right,
            Motion::Up,
            Motion::Down,
            Motion::LineStart,
            Motion::LineFirstNonBlank,
            Motion::LineEnd,
            Motion::WordForward,
            Motion::WordBackward,
            Motion::WordEnd,
            Motion::Top,
            Motion::Bottom,
            Motion::HalfPageUp,
            Motion::HalfPageDown,
            Motion::PageUp,
            Motion::PageDown,
        ];

        for motion in motions {
            let result = apply(motion, &scrollback, Cursor::new(3, 5), 10);
            assert_eq!(
                result,
                Cursor::new(0, 0),
                "{motion:?} on an empty scrollback"
            );
        }
    }
}
