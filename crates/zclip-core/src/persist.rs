//! Serializing and restoring a [`BufferRing`] across plugin reloads.
//!
//! Zellij can reload a plugin (e.g. `start-or-reload-plugin` during
//! development, or a Zellij restart) at any time, and without persistence
//! that wipes the whole paste-buffer ring -- a much worse experience than
//! tmux, whose paste buffers outlive any single client. This module is the
//! host-independent half of fixing that: a tiny, dependency-free text
//! format (`encode`/`decode`) plus the small pieces of naming and staleness
//! policy the host side needs to decide *where* and *whether* to persist.
//! Actually reading and writing bytes to a file is host glue and lives in
//! the sibling `zclip` crate; this module only turns a [`BufferRing`] into
//! a `String` and back.
//!
//! # Why a bespoke format instead of `serde` + JSON
//!
//! `zclip-core` is dependency-free by design (see `CONTRIBUTING.md`), so
//! pulling in `serde` -- even just for this one file -- is off the table.
//! The format below is deliberately simple: a one-line header followed by
//! one line per buffer, escaped just enough to make newlines and tabs safe
//! inside a line-oriented format. It is not meant to be a general-purpose
//! serialization scheme, only a faithful, round-trippable encoding of a
//! [`BufferRing`].
//!
//! # Format v1
//!
//! ```text
//! zclip-ring\t1
//! <seq>\t<name-field>\t<escaped-text>
//! <seq>\t<name-field>\t<escaped-text>
//! ...
//! ```
//!
//! * The header line is always exactly `zclip-ring` TAB `1`.
//! * Each record is `seq`, `name-field`, and `escaped-text`, tab-separated.
//! * `name-field` is `-` for an unnamed buffer, or `n` followed by the
//!   buffer's escaped name for a named one (so a bare `n` is a named
//!   buffer with an empty-string name, distinct from unnamed).
//! * Escaping applies to the name and the text, and nothing else:
//!   `\` becomes `\\`, LF becomes `\n`, CR becomes `\r`, and TAB becomes
//!   `\t`. Every other byte, including NUL, ESC, and multi-byte UTF-8,
//!   passes through unchanged. This is what keeps the format strictly
//!   line-oriented (a record's raw text never contains a literal newline
//!   or tab to be confused with a field separator) while never mangling
//!   the buffer's actual content.
//!
//! [`BufferId`](crate::BufferId) is deliberately not persisted: it is an
//! in-memory identity only, and writing it to disk would only invite
//! duplicate-id or `id >= next_id` corruption cases to guard against for no
//! benefit. The ring's `limit` is not persisted either -- the *live*
//! configuration is authoritative at load time, so a user who lowers
//! `buffer_limit` between reloads sees that take effect immediately rather
//! than being overridden by whatever limit happened to be in effect when
//! the file was written. `next_id` and `next_seq` are not persisted; they
//! are derived fresh on restore (see [`decode`]).
//!
//! `seq` is written but is advisory on read: [`decode`] sorts records by
//! seq descending and renumbers them to a fresh, dense range before
//! restoring. This sidesteps any `u64` overflow hazard a corrupt or
//! adversarial file's seq values could otherwise cause, and makes "seq is
//! strictly decreasing front-to-back" true by construction rather than by
//! trusting the file.

use std::error::Error;
use std::fmt;

use crate::buffer::RestoredBuffer;
use crate::{BufferRing, PasteBuffer};

/// The maximum encoded size of a single persisted buffer, in bytes.
///
/// A record whose tab-delimited, escaped encoding exceeds this many bytes
/// is left out of [`encode`]'s output entirely, and counted in
/// [`Encoded::skipped`]. It is never truncated: a half-restored buffer --
/// say, half of a shell script -- is more dangerous to paste than a
/// buffer that simply did not survive the reload.
pub const MAX_BUFFER_BYTES: usize = 1 << 20;

/// The maximum size of an entire persisted-ring file body, in bytes.
///
/// [`encode`] admits records until adding the next one would exceed this
/// budget. Named buffers are admitted first -- see [`encode`]'s doc
/// comment for why.
pub const MAX_TOTAL_BYTES: usize = 4 << 20;

/// How long, in seconds, a persisted-ring file may go unread before
/// [`is_stale`] considers it abandoned.
///
/// A week is long enough to survive a normal weekend away from a machine,
/// short enough that a ring file left behind by a `zellij_pid` that will
/// never come back again doesn't linger forever.
pub const STALE_AFTER_SECS: u64 = 7 * 24 * 60 * 60;

/// Whether -- and how -- the buffer ring is persisted to disk across
/// plugin reloads.
///
/// This is a user-facing configuration choice, not an internal detail:
/// persistence means writing paste-buffer contents to disk, which some
/// users may not want for sensitive clipboard data. Defaulting to
/// [`PersistMode::Off`] keeps that opt-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PersistMode {
    /// Never persist. The ring lives only in memory and is lost on every
    /// reload. This is the default.
    #[default]
    Off,
    /// Persist for the lifetime of the Zellij session: the ring survives
    /// plugin reloads, but is not expected to outlive the session itself.
    Session,
}

/// An error parsing a `persist` configuration value.
///
/// Carries the offending (trimmed) input, mirroring
/// [`BufferLimitError`](crate::BufferLimitError)'s approach to a helpful
/// error message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistModeError(String);

impl fmt::Display for PersistModeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "'{}' is not a valid persist (expected 'off' or 'session')",
            self.0
        )
    }
}

impl Error for PersistModeError {}

/// Parses a `persist` configuration value.
///
/// Accepts `"off"` and `"session"`, case-insensitively, after trimming
/// surrounding whitespace. Anything else is rejected as
/// [`PersistModeError`], carrying the trimmed input for a helpful error
/// message.
///
/// # Errors
///
/// Returns [`PersistModeError`] if `raw` is not (case-insensitively)
/// `"off"` or `"session"` once trimmed.
pub fn parse_persist_mode(raw: &str) -> Result<PersistMode, PersistModeError> {
    let trimmed = raw.trim();
    match trimmed.to_ascii_lowercase().as_str() {
        "off" => Ok(PersistMode::Off),
        "session" => Ok(PersistMode::Session),
        _ => Err(PersistModeError(trimmed.to_string())),
    }
}

/// The result of [`encode`]-ing a [`BufferRing`] to the on-disk format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encoded {
    /// The full file body: a header line followed by one line per
    /// admitted buffer.
    pub body: String,
    /// How many buffers were left out of `body`, whether because a single
    /// buffer's encoding exceeded [`MAX_BUFFER_BYTES`] or because the
    /// running total would have exceeded [`MAX_TOTAL_BYTES`].
    pub skipped: usize,
}

/// Escapes `\`, LF, CR, and TAB in `input`, leaving every other byte
/// (including NUL, ESC, and multi-byte UTF-8) untouched.
fn escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

/// Reverses [`escape`]. Returns `None` for a trailing lone backslash or an
/// escape sequence other than the four `escape` produces -- never panics,
/// never slices by byte index.
fn unescape(input: &str) -> Option<String> {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            _ => return None,
        }
    }
    Some(out)
}

/// Renders one buffer as a `<seq>\t<name-field>\t<escaped-text>` record
/// line, with no trailing newline.
fn encode_record(buffer: &PasteBuffer) -> String {
    let name_field = match buffer.name() {
        None => "-".to_string(),
        Some(name) => format!("n{}", escape(name)),
    };
    format!(
        "{}\t{}\t{}",
        buffer.seq(),
        name_field,
        escape(buffer.text())
    )
}

/// Encodes a [`BufferRing`] to the v1 text format described in the module
/// docs.
///
/// # Budget rules
///
/// A record whose encoded line exceeds [`MAX_BUFFER_BYTES`] is skipped
/// outright -- never truncated, since a partially-restored buffer is more
/// dangerous than a missing one. Of the remaining candidates, named
/// buffers are admitted first: the ring already exempts named buffers
/// from ordinary eviction because naming one means "I care about this",
/// and the persistence budget honours that same signal, so a pinned
/// buffer surviving a reload does not depend on how recently it was
/// pushed. Unnamed buffers then fill whatever budget remains, in ring
/// order (most-recent-first), until the next one would push the body past
/// [`MAX_TOTAL_BYTES`].
///
/// Regardless of that admission order, the records that *are* admitted are
/// always written out in ring order (most-recent-first): selection is
/// named-first, but the file itself should still read like the ring does.
///
/// [`Encoded::skipped`] counts every buffer left out, for either reason.
#[must_use]
pub fn encode(ring: &BufferRing) -> Encoded {
    const HEADER: &str = "zclip-ring\t1\n";

    struct Candidate<'a> {
        buffer: &'a PasteBuffer,
        line: String,
    }

    let mut skipped = 0usize;
    let candidates: Vec<Candidate<'_>> = ring
        .iter()
        .filter_map(|buffer| {
            let line = encode_record(buffer);
            if line.len() > MAX_BUFFER_BYTES {
                skipped += 1;
                None
            } else {
                Some(Candidate { buffer, line })
            }
        })
        .collect();

    let mut admitted = vec![false; candidates.len()];
    let mut used = HEADER.len();

    // Named buffers get first claim on the total budget.
    for (index, candidate) in candidates.iter().enumerate() {
        if candidate.buffer.is_named() {
            let needed = candidate.line.len() + 1;
            if used + needed <= MAX_TOTAL_BYTES {
                admitted[index] = true;
                used += needed;
            }
        }
    }
    // Unnamed buffers fill whatever budget remains, in ring order.
    for (index, candidate) in candidates.iter().enumerate() {
        if !candidate.buffer.is_named() {
            let needed = candidate.line.len() + 1;
            if used + needed <= MAX_TOTAL_BYTES {
                admitted[index] = true;
                used += needed;
            }
        }
    }

    let mut body = String::with_capacity(used);
    body.push_str(HEADER);
    for (index, candidate) in candidates.iter().enumerate() {
        if admitted[index] {
            body.push_str(&candidate.line);
            body.push('\n');
        } else {
            skipped += 1;
        }
    }

    Encoded { body, skipped }
}

/// The result of successfully [`decode`]-ing a persisted ring.
#[derive(Debug)]
pub struct Restored {
    /// The reconstructed ring, with every invariant re-established (see
    /// [`decode`]'s doc comment).
    pub ring: BufferRing,
    /// How many individual records in the file were malformed and
    /// skipped. A non-zero count does not fail the overall restore: a
    /// partial ring beats an empty one.
    pub skipped_records: usize,
}

/// Why a persisted-ring file could not be decoded at all.
///
/// This is distinct from a malformed *record*, which [`decode`] skips
/// individually and counts in [`Restored::skipped_records`] rather than
/// failing outright. A `DecodeError` means the file as a whole is not a
/// zclip ring file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// The input was empty or contained only whitespace.
    Empty,
    /// The first line was not the `zclip-ring` header.
    MissingHeader,
    /// The header named a format version other than `1`. Carries the
    /// offending version string (which may be empty, if the header had no
    /// second field at all).
    UnsupportedVersion(String),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "persisted ring is empty"),
            Self::MissingHeader => write!(f, "persisted ring is missing the 'zclip-ring' header"),
            Self::UnsupportedVersion(version) => {
                write!(f, "persisted ring has unsupported version '{version}'")
            }
        }
    }
}

impl Error for DecodeError {}

/// Parses one record line into `(seq, name, text)`.
///
/// Returns `None` for any malformed record: not exactly three
/// tab-separated fields, a non-numeric `seq`, a name-field whose leading
/// byte is neither `-` nor `n`, or an escape sequence `unescape` rejects.
/// Never panics.
fn parse_record(line: &str) -> Option<(u64, Option<String>, String)> {
    let fields: Vec<&str> = line.split('\t').collect();
    let [seq_field, name_field, text_field] = fields.as_slice() else {
        return None;
    };

    let seq: u64 = seq_field.parse().ok()?;

    let name = if *name_field == "-" {
        None
    } else {
        let rest = name_field.strip_prefix('n')?;
        Some(unescape(rest)?)
    };

    let text = unescape(text_field)?;

    Some((seq, name, text))
}

/// Decodes a persisted-ring file body and reconstructs a [`BufferRing`].
///
/// `limit` is the *current* configured `buffer_limit`, applied to the
/// restored ring via [`BufferRing::restore`] -- never whatever limit (if
/// any) happened to be in effect when the file was written, since that
/// value is not persisted in the first place.
///
/// Fails outright (see [`DecodeError`]) only if the input as a whole is
/// not a zclip ring file: empty, missing the header, or naming an
/// unsupported version. Individual malformed records are instead skipped
/// and counted in [`Restored::skipped_records`], including blank-text
/// records (which the ring's own public API can never produce, but a
/// hand-edited or corrupted file might contain) -- a partial restore beats
/// losing the whole ring over one bad line.
///
/// Never panics, on any input.
///
/// # Errors
///
/// Returns [`DecodeError`] if `body` is empty/whitespace-only, does not
/// start with the `zclip-ring` header, or names an unsupported version.
pub fn decode(body: &str, limit: usize) -> Result<Restored, DecodeError> {
    if body.trim().is_empty() {
        return Err(DecodeError::Empty);
    }

    let mut lines = body.lines();
    let Some(header_line) = lines.next() else {
        return Err(DecodeError::MissingHeader);
    };

    let mut header_fields = header_line.split('\t');
    if header_fields.next().unwrap_or_default() != "zclip-ring" {
        return Err(DecodeError::MissingHeader);
    }
    let version = header_fields.next().unwrap_or_default();
    if version != "1" {
        return Err(DecodeError::UnsupportedVersion(version.to_string()));
    }

    let mut skipped_records = 0usize;
    let mut records: Vec<(u64, Option<String>, String)> = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        match parse_record(line) {
            Some((seq, name, text)) if !text.trim().is_empty() => {
                records.push((seq, name, text));
            }
            _ => skipped_records += 1,
        }
    }

    // Advisory seq, descending: newest (as the file claims) first.
    records.sort_by_key(|record| std::cmp::Reverse(record.0));

    // Renumber to a fresh, dense range so a corrupt or adversarial seq
    // value (e.g. u64::MAX, or duplicates) can never cause an overflow or
    // ambiguity downstream -- see the module docs.
    let count = records.len();
    let entries: Vec<RestoredBuffer> = records
        .into_iter()
        .enumerate()
        .map(|(index, (_, name, text))| RestoredBuffer {
            seq: (count - index) as u64,
            name,
            text,
        })
        .collect();

    let ring = BufferRing::restore(limit, entries);
    Ok(Restored {
        ring,
        skipped_records,
    })
}

/// The persisted-ring file name for a given `zellij_pid`.
///
/// One file per Zellij server process, so that multiple concurrently
/// running Zellij sessions never clobber each other's ring.
#[must_use]
pub fn ring_file_name(zellij_pid: u32) -> String {
    format!("ring-{zellij_pid}.zclip")
}

/// The temporary file name used while atomically writing a plugin
/// instance's ring file.
///
/// Writing to a temp file and renaming it into place (the host's job, not
/// this crate's) avoids ever leaving a half-written, corrupt ring file
/// behind if the plugin is killed mid-write.
#[must_use]
pub fn temp_file_name(plugin_id: u32) -> String {
    format!("ring-{plugin_id}.tmp")
}

/// Whether `name` looks like a persisted-ring file (`ring-<digits>.zclip`),
/// as opposed to some other file that happens to share the directory.
///
/// Used by the host to decide which files in the persistence directory are
/// its own and which it should leave alone.
#[must_use]
pub fn is_ring_file(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("ring-") else {
        return false;
    };
    let Some(digits) = rest.strip_suffix(".zclip") else {
        return false;
    };
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

/// Whether a ring file last touched `age_secs` ago should be treated as
/// abandoned.
///
/// `age_secs` is exactly [`STALE_AFTER_SECS`] old is *not* stale yet --
/// only strictly older files are.
#[must_use]
pub fn is_stale(age_secs: u64) -> bool {
    age_secs > STALE_AFTER_SECS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::BufferRing;

    fn round_trip(ring: &BufferRing, limit: usize) -> Restored {
        let encoded = encode(ring);
        decode(&encoded.body, limit).expect("well-formed encode output must decode")
    }

    #[test]
    fn round_trip_empty_ring() {
        let ring = BufferRing::new(5);
        let encoded = encode(&ring);

        assert_eq!(encoded.skipped, 0);
        // An empty ring's body is just the header, which is whitespace-only
        // apart from the letters -- but decode must still treat "no
        // records" as a legitimate empty restore, not an error, since a
        // freshly-created ring is exactly this case.
        let restored = decode(&encoded.body, 5).unwrap();
        assert!(restored.ring.is_empty());
        assert_eq!(restored.skipped_records, 0);
    }

    #[test]
    fn round_trip_single_unnamed_buffer() {
        let mut ring = BufferRing::new(5);
        ring.push("hello world");

        let restored = round_trip(&ring, 5);

        assert_eq!(restored.ring.len(), 1);
        assert_eq!(restored.ring.get(0).unwrap().text(), "hello world");
        assert!(!restored.ring.get(0).unwrap().is_named());
    }

    #[test]
    fn round_trip_preserves_tricky_text() {
        let tricky = [
            "line one\nline two",
            "crlf\r\nhere",
            "a\ttab\there",
            "back\\slash",
            "literal two char sequence: \\n not a newline",
            "  leading and trailing whitespace  ",
            "nul\u{0}byte",
            "escape\u{1b}char",
            "日本語のテキスト",
            "emoji: \u{1F600}\u{1F601}",
            "combining: e\u{0301}\u{0301}",
        ];
        for text in tricky {
            let mut ring = BufferRing::new(5);
            ring.push(text);

            let restored = round_trip(&ring, 5);

            assert_eq!(
                restored.ring.get(0).unwrap().text(),
                text,
                "round-trip mismatch for {text:?}"
            );
        }
    }

    #[test]
    fn round_trip_name_with_tab_and_newline() {
        let mut ring = BufferRing::new(5);
        ring.push_named("text", "weird\tname\nhere");

        let restored = round_trip(&ring, 5);

        assert_eq!(
            restored.ring.get(0).unwrap().name(),
            Some("weird\tname\nhere")
        );
    }

    #[test]
    fn round_trip_empty_name_is_distinct_from_unnamed() {
        let mut ring = BufferRing::new(5);
        ring.push_named("named text", "");
        ring.push("unnamed text");

        let restored = round_trip(&ring, 5);

        let named = restored.ring.get_by_name("").unwrap();
        assert_eq!(named.text(), "named text");
        assert!(named.is_named());

        let unnamed = restored
            .ring
            .iter()
            .find(|b| b.text() == "unnamed text")
            .unwrap();
        assert!(!unnamed.is_named());
    }

    #[test]
    fn round_trip_format_injection_in_text_is_harmless() {
        let mut ring = BufferRing::new(5);
        ring.push("before");
        ring.push("zclip-ring\t1\n5\tn\\evil\tpwned");
        ring.push("after");

        let restored = round_trip(&ring, 5);

        assert_eq!(restored.ring.len(), 3);
        assert_eq!(
            restored.ring.get(1).unwrap().text(),
            "zclip-ring\t1\n5\tn\\evil\tpwned",
            "the malicious-looking text must survive verbatim, not be reinterpreted"
        );
    }

    #[test]
    fn scrambled_record_order_is_still_restored_by_recency() {
        let mut ring = BufferRing::new(5);
        ring.push("oldest");
        ring.push("middle");
        ring.push("newest");
        let encoded = encode(&ring);

        // Deliberately reverse the record lines (but keep the header
        // first), simulating a file whose records are out of seq order.
        let mut lines: Vec<&str> = encoded.body.lines().collect();
        let header = lines.remove(0);
        lines.reverse();
        let mut scrambled = String::new();
        scrambled.push_str(header);
        scrambled.push('\n');
        for line in lines {
            scrambled.push_str(line);
            scrambled.push('\n');
        }

        let restored = decode(&scrambled, 5).unwrap();
        let texts: Vec<&str> = restored.ring.iter().map(PasteBuffer::text).collect();
        assert_eq!(texts, vec!["newest", "middle", "oldest"]);
    }

    #[test]
    fn push_after_restore_lands_at_the_front_with_a_higher_seq() {
        let mut ring = BufferRing::new(5);
        ring.push("a");
        ring.push("b");
        let restored = round_trip(&ring, 5);

        let mut ring = restored.ring;
        let old_top_seq = ring.get(0).unwrap().seq();
        ring.push("c");

        assert_eq!(ring.get(0).unwrap().text(), "c");
        assert!(ring.get(0).unwrap().seq() > old_top_seq);
    }

    #[test]
    fn restored_ids_are_unique() {
        let mut ring = BufferRing::new(5);
        ring.push("a");
        ring.push("b");
        ring.push("c");
        let restored = round_trip(&ring, 5);

        let ids: Vec<_> = restored.ring.iter().map(crate::PasteBuffer::id).collect();
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(ids.len(), unique.len());
    }

    #[test]
    fn duplicate_names_collapse_to_the_newest() {
        // Hand-written body: two records genuinely share the name "clip"
        // (something `push_named`'s own overwrite semantics could never
        // produce, but a hand-edited or corrupted file might). The one
        // with the higher seq -- "second" -- must win.
        let body = "zclip-ring\t1\n1\tnclip\tfirst\n7\tnclip\tsecond\n";

        let restored = decode(body, 5).unwrap();

        assert_eq!(restored.ring.get_by_name("clip").unwrap().text(), "second");
        assert_eq!(
            restored
                .ring
                .iter()
                .filter(|b| b.name() == Some("clip"))
                .count(),
            1
        );
    }

    #[test]
    fn blank_text_record_is_dropped() {
        let body = "zclip-ring\t1\n5\t-\t   \n6\t-\tkept\n";
        let restored = decode(body, 5).unwrap();

        assert_eq!(restored.ring.len(), 1);
        assert_eq!(restored.ring.get(0).unwrap().text(), "kept");
        assert_eq!(restored.skipped_records, 1);
    }

    #[test]
    fn more_unnamed_than_limit_evicts_oldest_unnamed_via_real_enforce_limit() {
        let mut ring = BufferRing::new(10);
        for text in ["a", "b", "c", "d", "e"] {
            ring.push(text);
        }
        let encoded = encode(&ring);

        let restored = decode(&encoded.body, 2).unwrap();

        assert_eq!(restored.ring.len(), 2);
        let texts: Vec<&str> = restored.ring.iter().map(PasteBuffer::text).collect();
        assert_eq!(texts, vec!["e", "d"]);
    }

    #[test]
    fn named_count_alone_exceeding_limit_keeps_them_all() {
        let mut ring = BufferRing::new(10);
        ring.push_named("a", "n1");
        ring.push_named("b", "n2");
        ring.push_named("c", "n3");
        let encoded = encode(&ring);

        let restored = decode(&encoded.body, 1).unwrap();

        assert_eq!(restored.ring.len(), 3, "named buffers are never evicted");
    }

    #[test]
    fn decode_limit_comes_from_the_argument_not_the_file() {
        let mut ring = BufferRing::new(50);
        for text in ["a", "b", "c"] {
            ring.push(text);
        }
        let encoded = encode(&ring);

        let restored = decode(&encoded.body, 1).unwrap();

        assert_eq!(
            restored.ring.limit(),
            1,
            "the limit argument must win over whatever the ring had when saved"
        );
        assert_eq!(restored.ring.len(), 1);
    }

    #[test]
    fn decode_empty_input_is_an_error() {
        assert_eq!(decode("", 5).unwrap_err(), DecodeError::Empty);
    }

    #[test]
    fn decode_whitespace_only_input_is_an_error() {
        assert_eq!(decode("   \n\t \n", 5).unwrap_err(), DecodeError::Empty);
    }

    #[test]
    fn decode_missing_header_is_an_error() {
        assert_eq!(
            decode("not a header\n5\t-\ttext\n", 5).unwrap_err(),
            DecodeError::MissingHeader
        );
    }

    #[test]
    fn decode_unsupported_version_is_an_error() {
        assert_eq!(
            decode("zclip-ring\t2\n", 5).unwrap_err(),
            DecodeError::UnsupportedVersion("2".to_string())
        );
    }

    #[test]
    fn decode_header_with_no_version_is_an_error() {
        assert_eq!(
            decode("zclip-ring\n", 5).unwrap_err(),
            DecodeError::UnsupportedVersion(String::new())
        );
    }

    #[test]
    fn decode_truncated_final_line_without_trailing_newline_is_handled() {
        let body = "zclip-ring\t1\n5\t-\ttext";
        let restored = decode(body, 5).unwrap();
        assert_eq!(restored.ring.get(0).unwrap().text(), "text");
    }

    #[test]
    fn decode_record_with_too_few_fields_is_skipped() {
        let body = "zclip-ring\t1\n5\t-\n";
        let restored = decode(body, 5).unwrap();
        assert!(restored.ring.is_empty());
        assert_eq!(restored.skipped_records, 1);
    }

    #[test]
    fn decode_record_with_too_many_fields_is_skipped() {
        let body = "zclip-ring\t1\n5\t-\ttext\textra\n";
        let restored = decode(body, 5).unwrap();
        assert!(restored.ring.is_empty());
        assert_eq!(restored.skipped_records, 1);
    }

    #[test]
    fn decode_non_numeric_seq_is_skipped() {
        let body = "zclip-ring\t1\nabc\t-\ttext\n";
        let restored = decode(body, 5).unwrap();
        assert!(restored.ring.is_empty());
        assert_eq!(restored.skipped_records, 1);
    }

    #[test]
    fn decode_seq_of_u64_max_is_accepted_without_panicking() {
        let body = "zclip-ring\t1\n18446744073709551615\t-\ttext\n";
        let restored = decode(body, 5).unwrap();
        assert_eq!(restored.ring.len(), 1);
        assert_eq!(restored.skipped_records, 0);
    }

    #[test]
    fn decode_dangling_trailing_backslash_is_skipped() {
        let body = "zclip-ring\t1\n5\t-\ttext\\\n";
        let restored = decode(body, 5).unwrap();
        assert!(restored.ring.is_empty());
        assert_eq!(restored.skipped_records, 1);
    }

    #[test]
    fn decode_unknown_escape_is_skipped() {
        let body = "zclip-ring\t1\n5\t-\ttext\\qmore\n";
        let restored = decode(body, 5).unwrap();
        assert!(restored.ring.is_empty());
        assert_eq!(restored.skipped_records, 1);
    }

    #[test]
    fn decode_name_field_with_bad_leading_byte_is_skipped() {
        let body = "zclip-ring\t1\n5\tx\ttext\n";
        let restored = decode(body, 5).unwrap();
        assert!(restored.ring.is_empty());
        assert_eq!(restored.skipped_records, 1);
    }

    #[test]
    fn decode_handles_one_very_long_line_without_panicking() {
        let long_text = "a".repeat(1_000_000);
        let body = format!("zclip-ring\t1\n5\t-\t{long_text}\n");
        let restored = decode(&body, 5).unwrap();
        assert_eq!(restored.ring.get(0).unwrap().text().len(), 1_000_000);
    }

    #[test]
    fn encode_skips_a_buffer_whose_encoding_exceeds_the_per_buffer_cap() {
        let mut ring = BufferRing::new(10);
        ring.push("before");
        ring.push("x".repeat(MAX_BUFFER_BYTES + 10));
        ring.push("after");

        let encoded = encode(&ring);

        assert_eq!(encoded.skipped, 1);
        assert!(!encoded.body.contains(&"x".repeat(100)));
        let restored = decode(&encoded.body, 10).unwrap();
        assert_eq!(restored.ring.len(), 2);
        let texts: Vec<&str> = restored.ring.iter().map(PasteBuffer::text).collect();
        assert_eq!(texts, vec!["after", "before"]);
    }

    #[test]
    fn encode_named_first_budget_keeps_pinned_buffer_over_a_newer_unnamed_one() {
        let mut ring = BufferRing::new(10);
        // Named, oldest -- pinned, and must survive the budget cut.
        ring.push_named("n".repeat(850_000), "pinned");
        // Four unnamed, newer than the pinned buffer. Together with the
        // pinned buffer these exceed MAX_TOTAL_BYTES, so at least one must
        // be dropped despite being newer than the pinned buffer.
        ring.push("u1".to_string() + &"a".repeat(850_000));
        ring.push("u2".to_string() + &"b".repeat(850_000));
        ring.push("u3".to_string() + &"c".repeat(850_000));
        ring.push("u4".to_string() + &"d".repeat(850_000));

        let encoded = encode(&ring);

        assert!(encoded.body.len() <= MAX_TOTAL_BYTES);
        assert!(encoded.skipped >= 1);

        let restored = decode(&encoded.body, 10).unwrap();
        assert!(
            restored.ring.get_by_name("pinned").is_some(),
            "the pinned buffer must survive the budget cut"
        );
        assert!(
            restored.ring.len() < 5,
            "at least one newer unnamed buffer must have been dropped"
        );
    }

    #[test]
    fn encode_skipped_count_matches_omitted_buffers() {
        let mut ring = BufferRing::new(10);
        for text in ["a", "b", "c"] {
            ring.push(text);
        }
        ring.push("x".repeat(MAX_BUFFER_BYTES + 1));

        let encoded = encode(&ring);
        assert_eq!(encoded.skipped, 1);
    }

    #[test]
    fn parse_persist_mode_accepts_mixed_case_with_whitespace() {
        assert_eq!(parse_persist_mode("  OFF  "), Ok(PersistMode::Off));
        assert_eq!(parse_persist_mode("Session"), Ok(PersistMode::Session));
        assert_eq!(parse_persist_mode("session"), Ok(PersistMode::Session));
        assert_eq!(parse_persist_mode("off"), Ok(PersistMode::Off));
    }

    #[test]
    fn parse_persist_mode_rejects_garbage_and_carries_the_input() {
        let err = parse_persist_mode("always").unwrap_err();
        assert_eq!(err, PersistModeError("always".to_string()));
        assert!(err.to_string().contains("always"));
    }

    #[test]
    fn persist_mode_default_is_off() {
        assert_eq!(PersistMode::default(), PersistMode::Off);
    }

    #[test]
    fn ring_file_name_format() {
        assert_eq!(ring_file_name(1234), "ring-1234.zclip");
    }

    #[test]
    fn temp_file_name_format() {
        assert_eq!(temp_file_name(42), "ring-42.tmp");
    }

    #[test]
    fn is_ring_file_accepts_well_formed_names() {
        assert!(is_ring_file("ring-0.zclip"));
        assert!(is_ring_file("ring-1234.zclip"));
    }

    #[test]
    fn is_ring_file_rejects_malformed_names() {
        assert!(!is_ring_file("ring-.zclip"));
        assert!(!is_ring_file("ring-abc.zclip"));
        assert!(!is_ring_file("ring-12.zclip.tmp"));
        assert!(!is_ring_file("ring-12.tmp"));
        assert!(!is_ring_file("other-12.zclip"));
    }

    #[test]
    fn is_stale_boundary_is_exclusive() {
        assert!(!is_stale(STALE_AFTER_SECS));
        assert!(is_stale(STALE_AFTER_SECS + 1));
        assert!(!is_stale(0));
    }
}
