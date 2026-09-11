//! Config-driven keymaps for copy mode.
//!
//! Copy mode's behaviour -- which key applies which [`Motion`], which key
//! toggles which [`SelectionMode`], which key yanks, which key cancels -- has
//! historically been hardcoded in the Zellij-facing plugin crate, one match
//! arm per key. That is fine for a single, fixed vocabulary, but it cannot
//! support a user who wants Emacs-style bindings instead of vi's, or who just
//! wants to move `yank` off `y`.
//!
//! This module is the host-independent half of fixing that: it models
//! *which actions exist* ([`CopyModeAction`], the action-name vocabulary),
//! *what a preset binds them to* ([`VI_PRESET`], [`EMACS_PRESET`]), and *how
//! a user's configuration overrides a preset* ([`resolve_keymap`]). None of
//! that requires knowing what a key actually is -- a key is just the `&str`
//! spelling a user typed into their config (`"Ctrl d"`, `"y, Enter"`) -- so
//! this module treats keys as plain strings throughout. Parsing those
//! strings into whatever key type the host understands is the Zellij-facing
//! plugin crate's job, exactly as [`crate::motion`]'s docs describe for
//! motions themselves: keeping that translation out of here is what makes
//! every preset and every override rule in this module testable with
//! nothing but plain data.

use std::collections::BTreeMap;

use crate::motion::Motion;
use crate::region::SelectionMode;

/// A single thing a key can do in copy mode.
///
/// This wraps [`Motion`] and [`SelectionMode`] rather than duplicating their
/// variants, so a motion added to [`crate::motion`] cannot silently drift out
/// of sync with what the keymap can bind -- it is the same enum, one level
/// up. [`CopyModeAction::Yank`] and [`CopyModeAction::Cancel`] have no
/// existing home elsewhere in the crate, since neither is a cursor movement
/// or a selection shape; they live here instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyModeAction {
    /// Move the cursor. See [`Motion`] for the specific movement.
    Motion(Motion),
    /// Toggle a selection shape. See [`SelectionMode`] for which shape.
    Select(SelectionMode),
    /// Copy the current selection into the paste-buffer ring and leave copy
    /// mode.
    Yank,
    /// Copy the current selection into the paste-buffer ring *and* the
    /// system clipboard, then leave copy mode.
    ///
    /// A separate action rather than a `bool` on [`CopyModeAction::Yank`],
    /// because the keymap binds *actions* to keys: a flag would have no key
    /// spec of its own, so there would be no way for a preset to offer both
    /// on two different keys, nor for a `key_<action>` entry to move either
    /// one independently. The ring is written by both -- reaching the system
    /// clipboard is strictly additive, never instead of the ring -- so a
    /// clipboard yank is still findable in the buffer list afterwards
    /// exactly like a plain one.
    YankToClipboard,
    /// Back out one level: clear an active selection, or leave copy mode
    /// entirely if nothing is selected.
    Cancel,
}

/// The full vocabulary of action names a configuration may bind a key to,
/// paired with the [`CopyModeAction`] each name refers to.
///
/// This is the single source of truth for both directions of the
/// name-to-action mapping ([`action_by_name`] and [`action_name`]), so the
/// vocabulary can never drift between "what a `key_<name>` config entry may
/// say" and "what a preset table actually binds".
const ACTION_NAMES: &[(&str, CopyModeAction)] = &[
    ("left", CopyModeAction::Motion(Motion::Left)),
    ("down", CopyModeAction::Motion(Motion::Down)),
    ("up", CopyModeAction::Motion(Motion::Up)),
    ("right", CopyModeAction::Motion(Motion::Right)),
    ("word_forward", CopyModeAction::Motion(Motion::WordForward)),
    (
        "word_backward",
        CopyModeAction::Motion(Motion::WordBackward),
    ),
    ("word_end", CopyModeAction::Motion(Motion::WordEnd)),
    ("line_start", CopyModeAction::Motion(Motion::LineStart)),
    (
        "line_first_non_blank",
        CopyModeAction::Motion(Motion::LineFirstNonBlank),
    ),
    ("line_end", CopyModeAction::Motion(Motion::LineEnd)),
    ("top", CopyModeAction::Motion(Motion::Top)),
    ("bottom", CopyModeAction::Motion(Motion::Bottom)),
    ("half_page_up", CopyModeAction::Motion(Motion::HalfPageUp)),
    (
        "half_page_down",
        CopyModeAction::Motion(Motion::HalfPageDown),
    ),
    ("page_up", CopyModeAction::Motion(Motion::PageUp)),
    ("page_down", CopyModeAction::Motion(Motion::PageDown)),
    ("select_char", CopyModeAction::Select(SelectionMode::Char)),
    ("select_line", CopyModeAction::Select(SelectionMode::Line)),
    ("select_block", CopyModeAction::Select(SelectionMode::Block)),
    ("yank", CopyModeAction::Yank),
    ("yank_clipboard", CopyModeAction::YankToClipboard),
    ("cancel", CopyModeAction::Cancel),
];

/// Looks up the [`CopyModeAction`] a config action name refers to.
///
/// Returns `None` for any name outside the fixed vocabulary in
/// [`ACTION_NAMES`] -- callers (namely [`resolve_keymap`]) treat that as a
/// configuration mistake to warn about, not a panic.
#[must_use]
pub fn action_by_name(name: &str) -> Option<CopyModeAction> {
    ACTION_NAMES
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, action)| *action)
}

/// The canonical config action name for a [`CopyModeAction`].
///
/// Every variant of [`CopyModeAction`] that can exist has exactly one entry
/// in [`ACTION_NAMES`] (see the tests below for a check that both preset
/// tables only ever bind actions the vocabulary knows about), so this never
/// fails in practice; it still returns `Option` rather than panicking
/// because nothing about the type system proves that invariant.
#[must_use]
pub fn action_name(action: CopyModeAction) -> Option<&'static str> {
    ACTION_NAMES
        .iter()
        .find(|(_, candidate)| *candidate == action)
        .map(|(name, _)| *name)
}

/// vi's copy-mode keybindings, reproduced exactly as they were previously
/// hardcoded in the Zellij-facing plugin crate's `motion_for` and
/// `handle_copy_mode_key` functions.
///
/// This is a refactor, not a redesign: every key spec here, including the
/// arrow-key/Home/End/PageUp/PageDown aliases and the `Ctrl` paging keys,
/// must keep behaving exactly as it already does. `Ctrl b` is included even
/// though a comment at its original call site notes it is unreachable under
/// Zellij's default keybinds (Zellij's own `Tmux` mode claims `Ctrl b`
/// first) -- it is kept here regardless, because reproducing today's
/// behaviour means reproducing the dead code path too, not quietly dropping
/// it; a user who rebinds Zellij's default can still reach it.
pub const VI_PRESET: &[(CopyModeAction, &str)] = &[
    (CopyModeAction::Motion(Motion::Left), "h, Left"),
    (CopyModeAction::Motion(Motion::Down), "j, Down"),
    (CopyModeAction::Motion(Motion::Up), "k, Up"),
    (CopyModeAction::Motion(Motion::Right), "l, Right"),
    (CopyModeAction::Motion(Motion::WordForward), "w"),
    (CopyModeAction::Motion(Motion::WordBackward), "b"),
    (CopyModeAction::Motion(Motion::WordEnd), "e"),
    (CopyModeAction::Motion(Motion::LineStart), "0, Home"),
    (CopyModeAction::Motion(Motion::LineFirstNonBlank), "^"),
    (CopyModeAction::Motion(Motion::LineEnd), "$, End"),
    (CopyModeAction::Motion(Motion::Top), "g"),
    (CopyModeAction::Motion(Motion::Bottom), "G"),
    (CopyModeAction::Motion(Motion::HalfPageUp), "Ctrl u"),
    (CopyModeAction::Motion(Motion::HalfPageDown), "Ctrl d"),
    (CopyModeAction::Motion(Motion::PageUp), "Ctrl b, PageUp"),
    (CopyModeAction::Motion(Motion::PageDown), "Ctrl f, PageDown"),
    (CopyModeAction::Select(SelectionMode::Char), "v"),
    (CopyModeAction::Select(SelectionMode::Line), "V"),
    (CopyModeAction::Select(SelectionMode::Block), "Ctrl v"),
    (CopyModeAction::Yank, "y"),
    // vi's own "same operator, wider reach" convention: lowercase acts
    // locally, the shifted twin does the bigger thing. Mirrors the `"+y`
    // idea (yank to the system clipboard rather than the local register)
    // without needing vi's register-prefix grammar, which copy mode has no
    // equivalent of.
    (CopyModeAction::YankToClipboard, "Y"),
    (CopyModeAction::Cancel, "Esc"),
];

/// Emacs-style copy-mode keybindings.
///
/// Unlike [`VI_PRESET`], this is a fresh mapping rather than a refactor of
/// existing behaviour -- there is no prior Emacs preset to be faithful to --
/// so it simply follows Emacs's own well-known bindings (`Ctrl-b`/`Ctrl-f`/
/// `Ctrl-n`/`Ctrl-p` for motion, `Alt` for word- and page-level movement,
/// `Ctrl-Space` for starting a selection, `Ctrl-g` for "quit").
pub const EMACS_PRESET: &[(CopyModeAction, &str)] = &[
    (CopyModeAction::Motion(Motion::Left), "Ctrl b, Left"),
    (CopyModeAction::Motion(Motion::Right), "Ctrl f, Right"),
    (CopyModeAction::Motion(Motion::Down), "Ctrl n, Down"),
    (CopyModeAction::Motion(Motion::Up), "Ctrl p, Up"),
    (CopyModeAction::Motion(Motion::LineStart), "Ctrl a, Home"),
    (CopyModeAction::Motion(Motion::LineEnd), "Ctrl e, End"),
    (CopyModeAction::Motion(Motion::WordForward), "Alt f"),
    (CopyModeAction::Motion(Motion::WordBackward), "Alt b"),
    (CopyModeAction::Motion(Motion::WordEnd), "Alt e"),
    (CopyModeAction::Motion(Motion::Top), "Alt <"),
    (CopyModeAction::Motion(Motion::Bottom), "Alt >"),
    (CopyModeAction::Motion(Motion::PageDown), "Ctrl v, PageDown"),
    (CopyModeAction::Motion(Motion::PageUp), "Alt v, PageUp"),
    (CopyModeAction::Motion(Motion::HalfPageDown), "Ctrl d"),
    (CopyModeAction::Motion(Motion::HalfPageUp), "Ctrl u"),
    (CopyModeAction::Motion(Motion::LineFirstNonBlank), "Alt m"),
    (CopyModeAction::Select(SelectionMode::Char), "Ctrl Space"),
    (CopyModeAction::Select(SelectionMode::Line), "Alt l"),
    (CopyModeAction::Select(SelectionMode::Block), "Alt r"),
    (CopyModeAction::Yank, "Alt w"),
    // Emacs has no native "kill-ring-save, but to the system clipboard"
    // binding to be faithful to -- under a window system its kill ring *is*
    // the clipboard, so the distinction never arose. `Alt W` is therefore
    // chosen for the same reason vi's preset uses `Y`: the shifted twin of
    // the plain yank key, so the pair stays learnable as one rule across
    // both presets rather than two unrelated facts.
    (CopyModeAction::YankToClipboard, "Alt W"),
    (CopyModeAction::Cancel, "Ctrl g, Esc"),
];

/// Splits a comma-separated key spec (e.g. `"y, Enter"`) into its individual
/// key strings, trimming surrounding whitespace from each and dropping any
/// that are empty once trimmed.
///
/// # Why this is the *only* splitting logic, and why it lives here
///
/// This crate has no key parser -- that is `zclip`'s job, since parsing
/// `"Ctrl d"` into a real key type requires `zellij-tile`'s types, which
/// this crate must never depend on. That has one sharp consequence a caller
/// must respect: **a comma cannot be split on unconditionally**, because a
/// literal comma is itself a valid, bindable key (a user might reasonably
/// want to write `key_yank = ","`). Only `zclip`'s key parser can tell "a
/// spec that is the single key `,`" apart from "a spec listing two keys
/// separated by a comma" -- this module cannot, since to it both are just
/// bytes.
///
/// The contract this function relies on, therefore, is: **the caller must
/// first attempt to parse the whole trimmed `value` as one single key**, and
/// only fall back to calling this function -- and iterating its result --
/// if that whole-value parse fails. That ordering is what keeps a literal
/// `","` binding expressible at all, while still letting everyone else
/// write the readable, comma-separated multi-key form this function exists
/// to split.
#[must_use]
pub fn split_key_spec(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

/// Builds the effective copy-mode keymap from a plugin's configuration,
/// along with any warnings about configuration mistakes found along the
/// way.
///
/// # Preset selection
///
/// The `keymap` config key selects a preset: `"vi"` (the default, used when
/// `keymap` is absent) selects [`VI_PRESET`], `"emacs"` selects
/// [`EMACS_PRESET`]. Any other value is a configuration mistake: it produces
/// a warning naming both the bad value and the fallback being used, and
/// [`VI_PRESET`] is used, exactly as [`crate::parse_buffer_limit`]'s callers
/// fall back rather than refusing to start over one bad config entry.
///
/// # Per-action overrides
///
/// A `key_<action>` entry (e.g. `key_yank = "Y"`) **replaces** that action's
/// preset key spec entirely; it does not add to it. This is deliberate, not
/// an oversight: a user who writes `key_yank = "Y"` is trying to *move*
/// yank off `y`, and if `y` silently kept yanking as well as `Y`, that
/// would look like the config was ignored. Merging bindings instead of
/// replacing them is the trap this module is careful to avoid.
///
/// A `key_<action>` whose `<action>` is not one of the names
/// [`action_by_name`] recognises is itself a configuration mistake: it
/// produces a warning naming the offending config key and is otherwise
/// skipped, leaving that action's preset binding untouched.
///
/// # Returns
///
/// A list of `(action, key spec)` pairs -- one per action in the selected
/// preset, in the preset's own order, with any overrides applied -- and a
/// list of human-readable warning strings. The key spec is returned as the
/// raw, not-yet-split configuration string; see [`split_key_spec`] for why
/// splitting it is the caller's job.
#[must_use]
pub fn resolve_keymap(
    config: &BTreeMap<String, String>,
) -> (Vec<(CopyModeAction, String)>, Vec<String>) {
    let mut warnings = Vec::new();

    let preset = match config.get("keymap").map(String::as_str) {
        None | Some("vi") => VI_PRESET,
        Some("emacs") => EMACS_PRESET,
        Some(other) => {
            warnings.push(format!(
                "unknown keymap '{other}' (expected 'vi' or 'emacs'), using vi"
            ));
            VI_PRESET
        }
    };

    let mut bindings: Vec<(CopyModeAction, String)> = preset
        .iter()
        .map(|(action, spec)| (*action, (*spec).to_string()))
        .collect();

    for (config_key, raw_spec) in config {
        let Some(action_word) = config_key.strip_prefix("key_") else {
            continue;
        };
        match action_by_name(action_word) {
            Some(action) => {
                // Replace, not merge -- see the doc comment above for why an
                // override must fully take over the action's binding.
                if let Some(existing) = bindings.iter_mut().find(|(a, _)| *a == action) {
                    existing.1 = raw_spec.clone();
                } else {
                    bindings.push((action, raw_spec.clone()));
                }
            }
            None => {
                warnings.push(format!(
                    "unknown copy-mode action '{action_word}' in '{config_key}', ignoring"
                ));
            }
        }
    }

    (bindings, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    fn spec_for(bindings: &[(CopyModeAction, String)], action: CopyModeAction) -> &str {
        bindings
            .iter()
            .find(|(a, _)| *a == action)
            .map(|(_, spec)| spec.as_str())
            .unwrap_or_else(|| panic!("no binding for {action:?}"))
    }

    #[test]
    fn every_action_name_round_trips() {
        for (name, action) in ACTION_NAMES {
            assert_eq!(
                action_by_name(name),
                Some(*action),
                "action_by_name({name})"
            );
            assert_eq!(action_name(*action), Some(*name), "action_name({action:?})");
        }
    }

    #[test]
    fn every_preset_binding_is_in_the_vocabulary() {
        // Guards against a preset entry silently referring to an action that
        // no `key_<action>` config key could ever reach.
        for (action, _) in VI_PRESET.iter().chain(EMACS_PRESET) {
            assert!(
                action_name(*action).is_some(),
                "{action:?} has no vocabulary name"
            );
        }
    }

    #[test]
    fn absent_keymap_defaults_to_vi() {
        let (bindings, warnings) = resolve_keymap(&config(&[]));
        assert!(warnings.is_empty());
        assert_eq!(
            spec_for(&bindings, CopyModeAction::Motion(Motion::Down)),
            "j, Down"
        );
    }

    #[test]
    fn keymap_emacs_selects_the_emacs_table() {
        let (bindings, warnings) = resolve_keymap(&config(&[("keymap", "emacs")]));
        assert!(warnings.is_empty());
        assert_eq!(
            spec_for(&bindings, CopyModeAction::Motion(Motion::Down)),
            "Ctrl n, Down"
        );
    }

    #[test]
    fn unknown_keymap_value_falls_back_to_vi_and_warns() {
        let (bindings, warnings) = resolve_keymap(&config(&[("keymap", "dvorak")]));
        assert_eq!(
            spec_for(&bindings, CopyModeAction::Motion(Motion::Down)),
            "j, Down",
            "must fall back to vi"
        );
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("dvorak"), "must name the bad value");
    }

    #[test]
    fn key_override_replaces_the_preset_spec_rather_than_adding_to_it() {
        let (bindings, warnings) = resolve_keymap(&config(&[("key_yank", "Y")]));
        assert!(warnings.is_empty());
        assert_eq!(
            spec_for(&bindings, CopyModeAction::Yank),
            "Y",
            "moving yank off 'y' must not leave 'y' still bound"
        );
    }

    #[test]
    fn key_override_leaves_every_other_action_at_its_preset_binding() {
        let (bindings, _warnings) = resolve_keymap(&config(&[("key_yank", "Y")]));
        assert_eq!(
            spec_for(&bindings, CopyModeAction::Motion(Motion::Down)),
            "j, Down"
        );
        assert_eq!(spec_for(&bindings, CopyModeAction::Cancel), "Esc");
    }

    #[test]
    fn unknown_key_action_warns_and_is_skipped() {
        let (bindings, warnings) = resolve_keymap(&config(&[("key_frobnicate", "Z")]));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("frobnicate"), "must name the offender");
        assert!(
            bindings.iter().all(|(_, spec)| spec != "Z"),
            "an unrecognised action must not appear in the output"
        );
    }

    #[test]
    fn split_key_spec_handles_a_two_key_comma_list() {
        assert_eq!(
            split_key_spec("y, Enter"),
            vec!["y".to_string(), "Enter".to_string()]
        );
    }

    #[test]
    fn split_key_spec_handles_a_single_key() {
        assert_eq!(split_key_spec("y"), vec!["y".to_string()]);
    }

    #[test]
    fn split_key_spec_trims_ragged_whitespace() {
        assert_eq!(
            split_key_spec("  y  ,   Enter   ,Ctrl d "),
            vec!["y".to_string(), "Enter".to_string(), "Ctrl d".to_string()]
        );
    }

    #[test]
    fn split_key_spec_on_empty_string_is_empty() {
        assert!(split_key_spec("").is_empty());
    }

    #[test]
    fn split_key_spec_drops_empty_parts_from_stray_commas() {
        assert_eq!(
            split_key_spec("y,,Enter"),
            vec!["y".to_string(), "Enter".to_string()]
        );
        assert_eq!(split_key_spec(",,"), Vec::<String>::new());
    }
}
