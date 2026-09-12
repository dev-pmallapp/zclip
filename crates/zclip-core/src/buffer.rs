//! The paste-buffer ring: zclip's core data model.
//!
//! This is deliberately modelled on tmux's paste-buffer stack (`set-buffer`,
//! `paste-buffer`, `buffer-limit`), because tmux users already have working
//! muscle memory for it and its semantics have been battle-tested for
//! decades. The ring stores the most recently captured text at the front
//! (index `0`), like a stack.
//!
//! # The named-buffer exemption
//!
//! [`BufferRing::limit`] bounds the number of **unnamed** (automatic)
//! buffers only. Named buffers are pinned: they never count toward the
//! limit and are never evicted by [`BufferRing::push`]. This mirrors tmux,
//! where naming a buffer (`set-buffer -b`) is how you protect something you
//! care about from being pushed out by ordinary copies.
//!
//! A consequence: a ring with `limit = 3` holding 3 unnamed buffers and 5
//! named ones has `len() == 8`. Pushing another unnamed buffer evicts only
//! the oldest *unnamed* buffer, leaving all 5 named ones untouched. Naming a
//! buffer therefore frees up an eviction slot; un-naming one may push the
//! ring over capacity, in which case the limit is re-enforced immediately
//! (oldest-unnamed-first) rather than waiting for the next push.

use std::collections::{HashSet, VecDeque};
use std::error::Error;
use std::fmt;

/// tmux's default `buffer-limit`, used when no configuration overrides it.
pub const DEFAULT_BUFFER_LIMIT: usize = 50;

/// An opaque, stable identity for a [`PasteBuffer`].
///
/// Unlike a positional index (which shifts as the ring rotates), a
/// `BufferId` is assigned once at creation and never changes or gets
/// reused, so callers (e.g. a UI holding a selection) can keep referring to
/// "that one buffer" safely across pushes and evictions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BufferId(u64);

impl fmt::Display for BufferId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A single captured paste buffer.
///
/// Modelled directly on a tmux paste buffer: some text, an optional name
/// that pins it against eviction, and bookkeeping (`id`, `seq`) that lets
/// callers refer to it stably and order it by creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteBuffer {
    id: BufferId,
    text: String,
    name: Option<String>,
    seq: u64,
}

impl PasteBuffer {
    /// This buffer's stable identity.
    #[must_use]
    pub fn id(&self) -> BufferId {
        self.id
    }

    /// The buffer's full, untrimmed text, exactly as captured.
    ///
    /// zclip never trims or otherwise mutates stored text: whitespace that
    /// was part of the original selection is meaningful to the user (e.g.
    /// leading indentation) and it is not this crate's place to guess
    /// otherwise. Trimming, if wanted, is a UI-layer concern — see
    /// [`PasteBuffer::preview`] for the one case where zclip does collapse
    /// whitespace, for display purposes only.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The user-assigned name, if any.
    ///
    /// A named buffer is pinned against automatic eviction; see the module
    /// docs for details.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// The creation order of this buffer, monotonically increasing.
    ///
    /// Lower `seq` means older. This is distinct from [`BufferId`]: `seq` is
    /// meant for ordering comparisons, while `BufferId` is meant for
    /// identity lookups.
    #[must_use]
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// Whether this buffer currently has a name, i.e. is pinned against
    /// eviction.
    #[must_use]
    pub fn is_named(&self) -> bool {
        self.name.is_some()
    }

    /// The number of lines in the buffer's text.
    ///
    /// A trailing newline does not count as introducing an extra, empty
    /// line, matching how editors report line counts: `"a\nb\n"` is 2
    /// lines, not 3.
    #[must_use]
    pub fn line_count(&self) -> usize {
        if self.text.is_empty() {
            return 0;
        }
        let ends_with_newline = self.text.ends_with('\n');
        let newline_count = self.text.matches('\n').count();
        if ends_with_newline {
            newline_count
        } else {
            newline_count + 1
        }
    }

    /// A single-line preview of the buffer's text, suitable for a UI list.
    ///
    /// All whitespace runs (including newlines and tabs) are collapsed to a
    /// single space and the result is trimmed, so multi-line text renders
    /// as one readable line. If the collapsed text is longer than
    /// `max_chars` characters, it is truncated and a trailing `…` is
    /// appended so the total length (in `char`s) is exactly `max_chars`.
    ///
    /// Truncation always happens on a `char` boundary: this never slices
    /// through a multi-byte UTF-8 codepoint, unlike naive byte slicing.
    #[must_use]
    pub fn preview(&self, max_chars: usize) -> String {
        let mut collapsed = String::with_capacity(self.text.len());
        let mut last_was_space = false;
        for ch in self.text.chars() {
            if ch.is_whitespace() {
                if !last_was_space {
                    collapsed.push(' ');
                }
                last_was_space = true;
            } else {
                collapsed.push(ch);
                last_was_space = false;
            }
        }
        let trimmed = collapsed.trim();

        if max_chars == 0 {
            return String::new();
        }

        let char_count = trimmed.chars().count();
        if char_count <= max_chars {
            return trimmed.to_string();
        }

        let keep = max_chars.saturating_sub(1);
        let mut result: String = trimmed.chars().take(keep).collect();
        result.push('…');
        result
    }
}

/// An error parsing a `buffer_limit` configuration value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BufferLimitError {
    /// The value could not be parsed as a positive integer at all.
    ///
    /// Carries the original (trimmed) input, for a helpful error message.
    NotANumber(String),
    /// The value parsed as `0`.
    ///
    /// Rejected explicitly rather than silently clamped, because a ring
    /// that can hold nothing is not a configuration mistake worth guessing
    /// at — the user should be told.
    Zero,
}

impl fmt::Display for BufferLimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotANumber(raw) => {
                write!(
                    f,
                    "'{raw}' is not a valid buffer_limit (expected a positive integer)"
                )
            }
            Self::Zero => write!(f, "buffer_limit must be at least 1, got 0"),
        }
    }
}

impl Error for BufferLimitError {}

/// Parse a `buffer_limit` configuration value.
///
/// Surrounding whitespace is trimmed before parsing. Zero is rejected
/// explicitly (see [`BufferLimitError::Zero`]); anything that isn't a plain
/// non-negative integer (including negative numbers, decimals, and empty
/// strings) is rejected as [`BufferLimitError::NotANumber`].
///
/// # Errors
///
/// Returns [`BufferLimitError`] if `raw` does not parse to a positive
/// `usize`.
pub fn parse_buffer_limit(raw: &str) -> Result<usize, BufferLimitError> {
    let trimmed = raw.trim();
    match trimmed.parse::<usize>() {
        Ok(0) => Err(BufferLimitError::Zero),
        Ok(n) => Ok(n),
        Err(_) => Err(BufferLimitError::NotANumber(trimmed.to_string())),
    }
}

/// One buffer's worth of data recovered from a persisted-ring file, ready
/// to be handed to [`BufferRing::restore`].
///
/// Deliberately crate-private: [`crate::persist::decode`] is the only
/// producer of these, and [`BufferRing::restore`] is the only consumer.
/// Keeping both the type and the method out of the public API means
/// `BufferRing`'s four private fields stay private -- no external caller
/// can ever construct one of these and hand it to `restore` to build a
/// ring that violates an invariant the rest of this module maintains.
///
/// `seq` here is advisory only, used purely to work out recency order and
/// to break ties between duplicate names; the restored ring assigns its
/// own fresh `seq` (and `id`) to every buffer, exactly as [`BufferRing::push`]
/// does for a newly captured one.
pub(crate) struct RestoredBuffer {
    /// The buffer's `seq` as read from the file, already renumbered by
    /// [`crate::persist::decode`] to a fresh, dense range. Used only to
    /// order buffers and resolve duplicate names; never stored as-is.
    pub seq: u64,
    /// The buffer's name, if any, exactly as it will appear in the
    /// restored ring.
    pub name: Option<String>,
    /// The buffer's text, exactly as it will appear in the restored ring.
    pub text: String,
}

/// The tmux-style ring of paste buffers.
///
/// Backed by a [`VecDeque`] with the most recently pushed buffer at the
/// front (index `0`). See the module docs for the crucial detail that
/// [`BufferRing::limit`] only bounds unnamed buffers.
#[derive(Debug, Clone)]
pub struct BufferRing {
    buffers: VecDeque<PasteBuffer>,
    limit: usize,
    next_id: u64,
    next_seq: u64,
}

impl Default for BufferRing {
    /// Creates a ring using [`DEFAULT_BUFFER_LIMIT`], matching tmux's
    /// out-of-the-box behaviour.
    fn default() -> Self {
        Self::new(DEFAULT_BUFFER_LIMIT)
    }
}

impl BufferRing {
    /// Creates an empty ring holding at most `limit` unnamed buffers.
    ///
    /// A `limit` of `0` is clamped to `1` rather than accepted or panicking:
    /// a ring that could hold zero unnamed buffers would silently discard
    /// every ordinary copy, which is never useful and almost certainly a
    /// configuration mistake. Use [`parse_buffer_limit`] to reject `0`
    /// outright when the limit comes from user configuration.
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self {
            buffers: VecDeque::new(),
            limit: limit.max(1),
            next_id: 0,
            next_seq: 0,
        }
    }

    /// Rebuilds a ring from persisted data, re-establishing every
    /// invariant this module maintains rather than trusting the caller
    /// (i.e. [`crate::persist::decode`]) to have done so.
    ///
    /// `entries` need not already be in any particular order: this sorts
    /// by `seq` descending to determine recency, drops blank-text entries
    /// (the ring's own public API can never produce one, but a hand-edited
    /// or corrupted file might), and collapses duplicate names to the
    /// newest holder -- matching [`Self::push_named`]'s overwrite
    /// semantics. Every surviving entry is then assigned a fresh `id` and
    /// `seq` via the same [`Self::allocate_id`]/[`Self::allocate_seq`]
    /// helpers [`Self::push`] uses, so `next_id`/`next_seq` end up
    /// strictly greater than anything restored and no id is ever reused.
    ///
    /// Finally, `limit` -- the caller's *current* configuration, not
    /// anything read from the file -- is applied via the real
    /// [`Self::enforce_limit`], which evicts oldest-unnamed-first and
    /// never touches named buffers. This can leave `len() > limit` when
    /// named buffers are present, exactly as after any other operation on
    /// this ring; restoring never truncates to `limit`.
    pub(crate) fn restore(limit: usize, entries: Vec<RestoredBuffer>) -> Self {
        let mut entries: Vec<RestoredBuffer> = entries
            .into_iter()
            .filter(|entry| !entry.text.trim().is_empty())
            .collect();
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.seq));

        let mut seen_names: HashSet<String> = HashSet::new();
        entries.retain(|entry| match &entry.name {
            Some(name) => seen_names.insert(name.clone()),
            None => true,
        });

        let mut ring = Self::new(limit);
        // `entries` is most-recent-first; walk it in reverse (oldest
        // first) so that `allocate_seq` -- a plain increasing counter --
        // assigns higher seqs to more recent buffers, matching how `push`
        // grows `seq` over time. Pushing each to the front as we go
        // restores the most-recent-first order at the end.
        for entry in entries.into_iter().rev() {
            let id = ring.allocate_id();
            let seq = ring.allocate_seq();
            ring.buffers.push_front(PasteBuffer {
                id,
                text: entry.text,
                name: entry.name,
                seq,
            });
        }
        ring.enforce_limit();
        ring
    }

    /// The maximum number of **unnamed** buffers this ring will hold.
    ///
    /// Named buffers are pinned and do not count toward this limit: see the
    /// module docs for the full rationale. `len()` can therefore legitimately
    /// exceed `limit()` when named buffers are present.
    #[must_use]
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// The total number of buffers currently held, named and unnamed alike.
    #[must_use]
    pub fn len(&self) -> usize {
        self.buffers.len()
    }

    /// Whether the ring holds no buffers at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }

    fn allocate_id(&mut self) -> BufferId {
        let id = BufferId(self.next_id);
        self.next_id += 1;
        id
    }

    fn allocate_seq(&mut self) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        seq
    }

    /// Evicts oldest-unnamed-first until at most `limit` unnamed buffers
    /// remain.
    fn enforce_limit(&mut self) {
        while self.unnamed_count() > self.limit {
            let Some(victim_pos) = self
                .buffers
                .iter()
                .enumerate()
                .rev()
                .find(|(_, b)| !b.is_named())
                .map(|(pos, _)| pos)
            else {
                break;
            };
            self.buffers.remove(victim_pos);
        }
    }

    fn unnamed_count(&self) -> usize {
        self.buffers.iter().filter(|b| !b.is_named()).count()
    }

    /// Removes any existing buffer holding `name` (tmux `set-buffer -b`
    /// overwrite semantics: a name can only ever belong to one buffer).
    ///
    /// `keep` exempts one buffer from removal. This matters for
    /// [`Self::set_name`]: re-assigning a buffer the name it already holds
    /// must be a no-op, not a self-destruct.
    fn remove_by_name_except(&mut self, name: &str, keep: Option<BufferId>) {
        self.buffers
            .retain(|b| Some(b.id) == keep || b.name.as_deref() != Some(name));
    }

    /// Pushes a new unnamed buffer at the front of the ring.
    ///
    /// Returns `None`, pushing nothing, if `text` is empty or
    /// whitespace-only: a blank selection is never worth remembering. Any
    /// other text is stored verbatim (never trimmed).
    ///
    /// If this push causes the unnamed count to exceed [`Self::limit`], the
    /// oldest unnamed buffer is evicted. Named buffers are never affected.
    pub fn push(&mut self, text: impl Into<String>) -> Option<BufferId> {
        let text = text.into();
        if text.trim().is_empty() {
            return None;
        }
        let id = self.allocate_id();
        let seq = self.allocate_seq();
        self.buffers.push_front(PasteBuffer {
            id,
            text,
            name: None,
            seq,
        });
        self.enforce_limit();
        Some(id)
    }

    /// Pushes a new named buffer at the front of the ring.
    ///
    /// Returns `None`, pushing nothing, if `text` is empty or
    /// whitespace-only, exactly like [`Self::push`].
    ///
    /// Implements tmux `set-buffer -b <name>` semantics: if a buffer already
    /// holds `name`, it is removed from the ring entirely and the new
    /// buffer takes over that name at the front. Named buffers bypass
    /// eviction, so pushing one never evicts anything.
    pub fn push_named(
        &mut self,
        text: impl Into<String>,
        name: impl Into<String>,
    ) -> Option<BufferId> {
        let text = text.into();
        if text.trim().is_empty() {
            return None;
        }
        let name = name.into();
        self.remove_by_name_except(&name, None);
        let id = self.allocate_id();
        let seq = self.allocate_seq();
        self.buffers.push_front(PasteBuffer {
            id,
            text,
            name: Some(name),
            seq,
        });
        Some(id)
    }

    /// Assigns `name` to an existing buffer, pinning it against eviction.
    ///
    /// If some *other* buffer already holds `name`, that other buffer is
    /// removed from the ring (same overwrite semantics as
    /// [`Self::push_named`]). Re-assigning a buffer the name it already holds
    /// is a no-op that still returns `true`. Naming a buffer frees up one
    /// eviction slot for the remaining unnamed buffers, but does not itself
    /// trigger eviction (there is nothing to evict as a result of pinning
    /// something).
    ///
    /// Returns `false` if no buffer with `id` exists, in which case nothing
    /// is changed.
    pub fn set_name(&mut self, id: BufferId, name: impl Into<String>) -> bool {
        if !self.buffers.iter().any(|b| b.id == id) {
            return false;
        }
        let name = name.into();
        // Exempt `id` itself, or renaming a buffer to its own name would
        // delete the very buffer we are about to rename.
        self.remove_by_name_except(&name, Some(id));
        let Some(buffer) = self.buffers.iter_mut().find(|b| b.id == id) else {
            return false;
        };
        buffer.name = Some(name);
        true
    }

    /// Removes the name from an existing buffer, making it eligible for
    /// automatic eviction again.
    ///
    /// Because un-naming can push the unnamed count above [`Self::limit`],
    /// the limit is re-enforced immediately (oldest-unnamed-first) rather
    /// than waiting for the next [`Self::push`].
    ///
    /// Returns `false` if no buffer with `id` exists.
    pub fn clear_name(&mut self, id: BufferId) -> bool {
        let Some(buffer) = self.buffers.iter_mut().find(|b| b.id == id) else {
            return false;
        };
        buffer.name = None;
        self.enforce_limit();
        true
    }

    /// Returns the buffer at `index`, most-recent-first (`0` is the most
    /// recently pushed buffer, matching tmux `paste-buffer -b` addressing).
    ///
    /// Returns `None` if `index` is out of range.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&PasteBuffer> {
        self.buffers.get(index)
    }

    /// The most recently pushed buffer, if any. Equivalent to `get(0)`.
    #[must_use]
    pub fn most_recent(&self) -> Option<&PasteBuffer> {
        self.buffers.front()
    }

    /// Looks up a buffer by its stable [`BufferId`].
    #[must_use]
    pub fn get_by_id(&self, id: BufferId) -> Option<&PasteBuffer> {
        self.buffers.iter().find(|b| b.id == id)
    }

    /// Looks up a buffer by name.
    #[must_use]
    pub fn get_by_name(&self, name: &str) -> Option<&PasteBuffer> {
        self.buffers
            .iter()
            .find(|b| b.name.as_deref() == Some(name))
    }

    /// Resolves a user-supplied buffer selector, as passed to `paste-buffer`.
    ///
    /// Tries `selector` as a buffer **name** first, then as a positional index
    /// (`0` = most recent). Names deliberately win on collision: a user who
    /// went to the trouble of naming a buffer `2` means *that* buffer, not
    /// whatever currently sits at position 2. Positional indices shift on every
    /// yank, so treating them as the fallback keeps the stable interpretation
    /// as the primary one.
    ///
    /// Returns `None` for an empty selector, an unknown name, or an index past
    /// the end of the ring.
    #[must_use]
    pub fn resolve(&self, selector: &str) -> Option<&PasteBuffer> {
        let selector = selector.trim();
        if selector.is_empty() {
            return None;
        }
        if let Some(buffer) = self.get_by_name(selector) {
            return Some(buffer);
        }
        selector
            .parse::<usize>()
            .ok()
            .and_then(|index| self.get(index))
    }

    /// Removes and returns the buffer with the given `id`, if present.
    ///
    /// This works regardless of whether the buffer is named, since it is an
    /// explicit removal rather than automatic eviction.
    pub fn remove(&mut self, id: BufferId) -> Option<PasteBuffer> {
        let pos = self.buffers.iter().position(|b| b.id == id)?;
        self.buffers.remove(pos)
    }

    /// Removes every buffer, named or not.
    ///
    /// This drops named buffers too: unlike automatic eviction, `clear` is
    /// an explicit user action ("empty everything"), so the naming
    /// protection does not apply.
    pub fn clear(&mut self) {
        self.buffers.clear();
    }

    /// Iterates over all buffers, most-recent-first.
    pub fn iter(&self) -> impl Iterator<Item = &PasteBuffer> {
        self.buffers.iter()
    }

    /// Changes the unnamed-buffer limit.
    ///
    /// A `limit` of `0` is clamped to `1`, for the same reason as
    /// [`Self::new`]. If the new limit is smaller than the current unnamed
    /// count, oldest-unnamed buffers are evicted immediately to bring the
    /// ring back into compliance; growing the limit never resurrects
    /// anything that was already evicted.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit.max(1);
        self.enforce_limit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_below_limit_keeps_everything() {
        let mut ring = BufferRing::new(5);
        ring.push("a");
        ring.push("b");
        ring.push("c");

        assert_eq!(ring.len(), 3);
        assert_eq!(ring.get(0).unwrap().text(), "c");
        assert_eq!(ring.get(1).unwrap().text(), "b");
        assert_eq!(ring.get(2).unwrap().text(), "a");
    }

    #[test]
    fn push_exactly_at_limit_keeps_everything() {
        let mut ring = BufferRing::new(3);
        ring.push("a");
        ring.push("b");
        ring.push("c");

        assert_eq!(ring.len(), 3);
    }

    #[test]
    fn push_above_limit_evicts_oldest_unnamed() {
        let mut ring = BufferRing::new(3);
        ring.push("a");
        ring.push("b");
        ring.push("c");
        ring.push("d");

        assert_eq!(ring.len(), 3);
        let texts: Vec<&str> = ring.iter().map(PasteBuffer::text).collect();
        assert_eq!(
            texts,
            vec!["d", "c", "b"],
            "a should have been evicted as oldest"
        );
    }

    #[test]
    fn eviction_keeps_the_n_most_recent_in_order() {
        let mut ring = BufferRing::new(2);
        for text in ["one", "two", "three", "four", "five"] {
            ring.push(text);
        }

        let texts: Vec<&str> = ring.iter().map(PasteBuffer::text).collect();
        assert_eq!(texts, vec!["five", "four"]);
    }

    #[test]
    fn named_buffers_survive_eviction_pressure() {
        let mut ring = BufferRing::new(2);
        let named_id = ring.push_named("keep me", "important").unwrap();
        ring.push("a");
        ring.push("b");
        ring.push("c");

        assert!(
            ring.get_by_id(named_id).is_some(),
            "named buffer must survive"
        );
        let unnamed_texts: Vec<&str> = ring
            .iter()
            .filter(|b| !b.is_named())
            .map(PasteBuffer::text)
            .collect();
        assert_eq!(
            unnamed_texts,
            vec!["c", "b"],
            "only the 2 most recent unnamed survive"
        );
        assert_eq!(ring.len(), 3);
    }

    #[test]
    fn all_named_ring_accepts_further_named_pushes_without_loss() {
        let mut ring = BufferRing::new(1);
        ring.push_named("first", "n1");
        ring.push_named("second", "n2");
        ring.push_named("third", "n3");

        assert_eq!(ring.len(), 3, "no named buffer should ever be evicted");
        assert!(ring.get_by_name("n1").is_some());
        assert!(ring.get_by_name("n2").is_some());
        assert!(ring.get_by_name("n3").is_some());
    }

    #[test]
    fn naming_a_buffer_frees_a_slot_for_unnamed_pushes() {
        let mut ring = BufferRing::new(2);
        let old_id = ring.push("old").unwrap();
        ring.push("newer");
        assert_eq!(ring.len(), 2);

        assert!(ring.set_name(old_id, "pinned"));
        ring.push("newest");

        assert_eq!(
            ring.len(),
            3,
            "naming freed a slot so nothing had to be evicted"
        );
        assert!(ring.get_by_id(old_id).is_some());
    }

    #[test]
    fn clear_name_reimposes_the_limit_immediately() {
        let mut ring = BufferRing::new(2);
        let pinned_id = ring.push_named("keep me", "keep").unwrap();
        ring.push("a");
        ring.push("b");
        assert_eq!(ring.len(), 3);

        assert!(ring.clear_name(pinned_id));

        assert_eq!(
            ring.len(),
            2,
            "un-naming must immediately re-enforce the limit"
        );
        assert!(
            ring.get_by_id(pinned_id).is_none(),
            "the un-named buffer was the oldest unnamed one now, by insertion order"
        );
    }

    #[test]
    fn push_named_with_existing_name_removes_prior_holder() {
        let mut ring = BufferRing::new(5);
        let first_id = ring.push_named("first text", "clip").unwrap();
        let second_id = ring.push_named("second text", "clip").unwrap();

        assert_eq!(
            ring.len(),
            1,
            "the old holder of the name must be gone, not just renamed"
        );
        assert!(ring.get_by_id(first_id).is_none());
        assert!(ring.get_by_id(second_id).is_some());
        assert_eq!(ring.get_by_name("clip").unwrap().text(), "second text");
    }

    #[test]
    fn set_name_with_existing_name_removes_prior_holder() {
        let mut ring = BufferRing::new(5);
        let first_id = ring.push_named("first", "clip").unwrap();
        let second_id = ring.push("second").unwrap();

        assert!(ring.set_name(second_id, "clip"));

        assert_eq!(ring.len(), 1);
        assert!(ring.get_by_id(first_id).is_none());
        assert_eq!(ring.get_by_name("clip").unwrap().id(), second_id);
    }

    #[test]
    fn renaming_a_buffer_to_its_own_name_does_not_destroy_it() {
        // Regression: `set_name` used to remove the prior holder of the name
        // before locating the target. When the target *was* the prior holder,
        // it deleted the buffer and returned false -- silent data loss on an
        // idempotent rename.
        let mut ring = BufferRing::new(5);
        let id = ring.push_named("precious", "clip").unwrap();

        assert!(ring.set_name(id, "clip"), "idempotent rename must succeed");
        assert_eq!(ring.len(), 1, "the buffer must still be there");
        assert_eq!(ring.get_by_id(id).unwrap().text(), "precious");
        assert_eq!(ring.get_by_name("clip").unwrap().id(), id);
    }

    #[test]
    fn renaming_a_named_buffer_releases_its_previous_name() {
        let mut ring = BufferRing::new(5);
        let id = ring.push_named("text", "old").unwrap();

        assert!(ring.set_name(id, "new"));

        assert_eq!(ring.len(), 1);
        assert!(
            ring.get_by_name("old").is_none(),
            "the old name must no longer resolve"
        );
        assert_eq!(ring.get_by_name("new").unwrap().id(), id);
    }

    #[test]
    fn get_zero_is_most_recent_and_ordering_matches_push_order() {
        let mut ring = BufferRing::new(5);
        ring.push("a");
        ring.push("b");
        ring.push("c");

        assert_eq!(ring.get(0).unwrap().text(), "c");
        assert_eq!(ring.most_recent().unwrap().text(), "c");
        assert_eq!(ring.get(1).unwrap().text(), "b");
        assert_eq!(ring.get(2).unwrap().text(), "a");
        assert!(ring.get(3).is_none());
    }

    #[test]
    fn empty_ring_get_and_most_recent_return_none() {
        let ring = BufferRing::new(5);

        assert!(ring.get(0).is_none());
        assert!(ring.most_recent().is_none());
        assert!(ring.is_empty());
    }

    #[test]
    fn blank_text_is_rejected() {
        let mut ring = BufferRing::new(5);

        assert!(ring.push("").is_none());
        assert!(ring.push("   ").is_none());
        assert!(ring.push("\n\t ").is_none());
        assert_eq!(ring.len(), 0);
    }

    #[test]
    fn blank_named_text_is_also_rejected() {
        let mut ring = BufferRing::new(5);

        assert!(ring.push_named("", "name").is_none());
        assert!(ring.push_named("   ", "name").is_none());
        assert_eq!(ring.len(), 0);
    }

    #[test]
    fn text_with_real_content_and_surrounding_whitespace_is_preserved_verbatim() {
        let mut ring = BufferRing::new(5);
        ring.push("  hello world  \n");

        assert_eq!(ring.get(0).unwrap().text(), "  hello world  \n");
    }

    #[test]
    fn parse_buffer_limit_accepts_valid_numbers() {
        assert_eq!(parse_buffer_limit("50"), Ok(50));
        assert_eq!(parse_buffer_limit("1"), Ok(1));
    }

    #[test]
    fn parse_buffer_limit_trims_surrounding_whitespace() {
        assert_eq!(parse_buffer_limit("  42  "), Ok(42));
        assert_eq!(parse_buffer_limit("\t7\n"), Ok(7));
    }

    #[test]
    fn parse_buffer_limit_rejects_zero() {
        assert_eq!(parse_buffer_limit("0"), Err(BufferLimitError::Zero));
    }

    #[test]
    fn parse_buffer_limit_rejects_non_numbers() {
        assert!(matches!(
            parse_buffer_limit("abc"),
            Err(BufferLimitError::NotANumber(_))
        ));
        assert!(matches!(
            parse_buffer_limit(""),
            Err(BufferLimitError::NotANumber(_))
        ));
        assert!(matches!(
            parse_buffer_limit("-1"),
            Err(BufferLimitError::NotANumber(_))
        ));
        assert!(matches!(
            parse_buffer_limit("3.5"),
            Err(BufferLimitError::NotANumber(_))
        ));
    }

    #[test]
    fn set_limit_shrinking_evicts_immediately() {
        let mut ring = BufferRing::new(5);
        for text in ["a", "b", "c", "d", "e"] {
            ring.push(text);
        }

        ring.set_limit(2);

        assert_eq!(ring.len(), 2);
        let texts: Vec<&str> = ring.iter().map(PasteBuffer::text).collect();
        assert_eq!(texts, vec!["e", "d"]);
    }

    #[test]
    fn set_limit_growing_does_not_resurrect_evicted_buffers() {
        let mut ring = BufferRing::new(2);
        for text in ["a", "b", "c"] {
            ring.push(text);
        }
        assert_eq!(ring.len(), 2);

        ring.set_limit(10);

        assert_eq!(
            ring.len(),
            2,
            "growing the limit must not bring back evicted buffers"
        );
    }

    #[test]
    fn set_limit_zero_is_clamped_to_one() {
        let mut ring = BufferRing::new(5);
        ring.set_limit(0);

        assert_eq!(ring.limit(), 1);
    }

    #[test]
    fn new_with_zero_limit_is_clamped_to_one() {
        let ring = BufferRing::new(0);

        assert_eq!(ring.limit(), 1);
    }

    #[test]
    fn buffer_ids_are_unique_and_never_reused_after_eviction() {
        let mut ring = BufferRing::new(1);
        let first_id = ring.push("a").unwrap();
        let second_id = ring.push("b").unwrap();

        assert_ne!(first_id, second_id);
        assert!(ring.get_by_id(first_id).is_none(), "a was evicted");

        let third_id = ring.push("c").unwrap();
        assert_ne!(
            third_id, first_id,
            "ids must never be reused, even after eviction"
        );
        assert_ne!(third_id, second_id);
    }

    #[test]
    fn preview_collapses_whitespace_runs() {
        let mut ring = BufferRing::new(5);
        ring.push("hello\n\nworld\t\tagain");

        assert_eq!(ring.get(0).unwrap().preview(100), "hello world again");
    }

    #[test]
    fn preview_truncates_with_ellipsis() {
        let mut ring = BufferRing::new(5);
        ring.push("abcdefghij");

        let preview = ring.get(0).unwrap().preview(5);
        assert_eq!(preview, "abcd…");
        assert_eq!(preview.chars().count(), 5);
    }

    #[test]
    fn preview_is_char_safe_on_multibyte_utf8() {
        let mut ring = BufferRing::new(5);
        ring.push("日本語のテキスト");

        let preview = ring.get(0).unwrap().preview(4);
        assert_eq!(preview, "日本語…");
        assert_eq!(preview.chars().count(), 4);
    }

    #[test]
    fn preview_does_not_truncate_when_it_fits() {
        let mut ring = BufferRing::new(5);
        ring.push("short");

        assert_eq!(ring.get(0).unwrap().preview(100), "short");
    }

    #[test]
    fn line_count_single_line() {
        let mut ring = BufferRing::new(5);
        ring.push("just one line");

        assert_eq!(ring.get(0).unwrap().line_count(), 1);
    }

    #[test]
    fn line_count_multi_line() {
        let mut ring = BufferRing::new(5);
        ring.push("line one\nline two\nline three");

        assert_eq!(ring.get(0).unwrap().line_count(), 3);
    }

    #[test]
    fn line_count_trailing_newline_does_not_add_a_phantom_line() {
        let mut ring = BufferRing::new(5);
        ring.push("line one\nline two\n");

        assert_eq!(ring.get(0).unwrap().line_count(), 2);
    }

    #[test]
    fn resolve_matches_a_buffer_name() {
        let mut ring = BufferRing::new(5);
        ring.push_named("pinned text", "notes");
        ring.push("other");

        assert_eq!(ring.resolve("notes").unwrap().text(), "pinned text");
    }

    #[test]
    fn resolve_falls_back_to_a_positional_index() {
        let mut ring = BufferRing::new(5);
        ring.push("oldest");
        ring.push("newest");

        assert_eq!(ring.resolve("0").unwrap().text(), "newest");
        assert_eq!(ring.resolve("1").unwrap().text(), "oldest");
    }

    #[test]
    fn resolve_prefers_a_name_over_a_colliding_index() {
        let mut ring = BufferRing::new(5);
        ring.push("position one");
        // Deliberately name a buffer "1" while another buffer sits at index 1.
        ring.push_named("named one", "1");

        assert_eq!(
            ring.resolve("1").unwrap().text(),
            "named one",
            "an explicit name must win over a positional index"
        );
    }

    #[test]
    fn resolve_trims_whitespace() {
        let mut ring = BufferRing::new(5);
        ring.push_named("text", "notes");

        assert!(ring.resolve("  notes  ").is_some());
    }

    #[test]
    fn resolve_rejects_empty_unknown_and_out_of_range_selectors() {
        let mut ring = BufferRing::new(5);
        ring.push("only");

        assert!(ring.resolve("").is_none());
        assert!(ring.resolve("   ").is_none());
        assert!(ring.resolve("nosuchname").is_none());
        assert!(ring.resolve("7").is_none());
        assert!(ring.resolve("-1").is_none());
    }

    #[test]
    fn remove_deletes_named_or_unnamed_buffers() {
        let mut ring = BufferRing::new(5);
        let unnamed_id = ring.push("a").unwrap();
        let named_id = ring.push_named("b", "keep").unwrap();

        assert!(ring.remove(unnamed_id).is_some());
        assert!(ring.remove(named_id).is_some());
        assert!(ring.is_empty());
    }

    #[test]
    fn clear_drops_named_and_unnamed_buffers() {
        let mut ring = BufferRing::new(5);
        ring.push("a");
        ring.push_named("b", "keep");

        ring.clear();

        assert!(
            ring.is_empty(),
            "clear is explicit and must drop named buffers too"
        );
    }

    #[test]
    fn is_named_reflects_current_state() {
        let mut ring = BufferRing::new(5);
        let id = ring.push("a").unwrap();

        assert!(!ring.get_by_id(id).unwrap().is_named());

        ring.set_name(id, "n");
        assert!(ring.get_by_id(id).unwrap().is_named());

        ring.clear_name(id);
        assert!(!ring.get_by_id(id).unwrap().is_named());
    }

    #[test]
    fn set_name_on_unknown_id_returns_false() {
        let mut ring = BufferRing::new(5);
        let unknown = {
            let mut throwaway = BufferRing::new(1);
            let id = throwaway.push("x").unwrap();
            throwaway.push("y");
            id
        };

        assert!(!ring.set_name(unknown, "whatever"));
    }

    #[test]
    fn clear_name_on_unknown_id_returns_false() {
        let mut ring = BufferRing::new(5);
        let unknown = {
            let mut throwaway = BufferRing::new(1);
            let id = throwaway.push("x").unwrap();
            throwaway.push("y");
            id
        };

        assert!(!ring.clear_name(unknown));
    }

    #[test]
    fn seq_is_monotonically_increasing_with_creation_order() {
        let mut ring = BufferRing::new(5);
        ring.push("a");
        ring.push("b");
        ring.push("c");

        let mut seqs: Vec<u64> = ring.iter().map(PasteBuffer::seq).collect();
        seqs.reverse();
        assert!(seqs.windows(2).all(|w| w[0] < w[1]));
    }

    fn restored(seq: u64, name: Option<&str>, text: &str) -> RestoredBuffer {
        RestoredBuffer {
            seq,
            name: name.map(str::to_string),
            text: text.to_string(),
        }
    }

    #[test]
    fn restore_orders_entries_most_recent_first_by_seq_regardless_of_input_order() {
        let ring = BufferRing::restore(
            5,
            vec![
                restored(1, None, "oldest"),
                restored(3, None, "newest"),
                restored(2, None, "middle"),
            ],
        );

        let texts: Vec<&str> = ring.iter().map(PasteBuffer::text).collect();
        assert_eq!(texts, vec!["newest", "middle", "oldest"]);
    }

    #[test]
    fn restore_drops_blank_text_entries() {
        let ring =
            BufferRing::restore(5, vec![restored(1, None, "   "), restored(2, None, "kept")]);

        assert_eq!(ring.len(), 1);
        assert_eq!(ring.get(0).unwrap().text(), "kept");
    }

    #[test]
    fn restore_collapses_duplicate_names_to_the_highest_seq() {
        let ring = BufferRing::restore(
            5,
            vec![
                restored(1, Some("clip"), "first"),
                restored(5, Some("clip"), "second"),
            ],
        );

        assert_eq!(ring.len(), 1);
        assert_eq!(ring.get_by_name("clip").unwrap().text(), "second");
    }

    #[test]
    fn restore_assigns_fresh_unique_ids() {
        let ring = BufferRing::restore(5, vec![restored(1, None, "a"), restored(2, None, "b")]);

        let ids: Vec<BufferId> = ring.iter().map(PasteBuffer::id).collect();
        assert_ne!(ids[0], ids[1]);
    }

    #[test]
    fn restore_assigns_fresh_seqs_with_the_most_recent_entry_highest() {
        let ring = BufferRing::restore(
            5,
            vec![restored(100, None, "newest"), restored(1, None, "oldest")],
        );

        assert!(ring.get(0).unwrap().seq() > ring.get(1).unwrap().seq());
    }

    #[test]
    fn restore_next_id_and_next_seq_are_strictly_greater_than_anything_restored() {
        let mut ring = BufferRing::restore(5, vec![restored(1, None, "a"), restored(2, None, "b")]);

        let existing_ids: Vec<BufferId> = ring.iter().map(PasteBuffer::id).collect();
        let existing_seqs: Vec<u64> = ring.iter().map(PasteBuffer::seq).collect();

        let new_id = ring.push("c").unwrap();
        assert!(!existing_ids.contains(&new_id));
        assert!(!existing_seqs.contains(&ring.get(0).unwrap().seq()));
    }

    #[test]
    fn restore_applies_the_argument_limit_evicting_oldest_unnamed_via_enforce_limit() {
        let ring = BufferRing::restore(
            2,
            vec![
                restored(1, None, "a"),
                restored(2, None, "b"),
                restored(3, None, "c"),
                restored(4, None, "d"),
            ],
        );

        assert_eq!(ring.len(), 2);
        let texts: Vec<&str> = ring.iter().map(PasteBuffer::text).collect();
        assert_eq!(texts, vec!["d", "c"]);
    }

    #[test]
    fn restore_never_evicts_named_buffers_even_when_they_alone_exceed_the_limit() {
        let ring = BufferRing::restore(
            1,
            vec![
                restored(1, Some("n1"), "a"),
                restored(2, Some("n2"), "b"),
                restored(3, Some("n3"), "c"),
            ],
        );

        assert_eq!(ring.len(), 3, "named buffers must never be evicted");
    }

    #[test]
    fn restore_len_can_legitimately_exceed_limit_when_named_buffers_are_present() {
        let ring = BufferRing::restore(
            1,
            vec![
                restored(1, Some("pinned"), "keep"),
                restored(2, None, "a"),
                restored(3, None, "b"),
            ],
        );

        assert_eq!(ring.len(), 2, "1 pinned + 1 unnamed within the limit of 1");
        assert!(ring.get_by_name("pinned").is_some());
    }

    #[test]
    fn restore_of_empty_entries_yields_an_empty_ring() {
        let ring = BufferRing::restore(5, Vec::new());
        assert!(ring.is_empty());
    }
}
