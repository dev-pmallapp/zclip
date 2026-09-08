---
title: Surface clipboard failures to the user
milestone: M3 - System Clipboard Bridge
labels: task,area:clipboard,priority:p1
---

## Context
Clipboard bridging can fail for many reasons: missing permission grants, no backend binary on `PATH`, an unsupported display server, or a shell-out command exiting non-zero. Silent failure is worse than no bridge at all, because the user believes text was copied when it wasn't. This task centralizes failure detection and presentation across both bridging paths (issue 31 and issue 33).

## Acceptance criteria
- [ ] `Event::SystemClipboardFailure` (from the `copy_to_clipboard` path, issue 31) is caught and turned into a user-visible message in the plugin UI.
- [ ] Non-zero exit codes or non-empty stderr from `Event::RunCommandResult` (from the shell-out path, issue 33) are caught and turned into a user-visible message, including the offending backend name and captured stderr text.
- [ ] Failures never crash the plugin or leave it in an inconsistent state; the in-app yank buffer remains intact regardless of bridge outcome.
- [ ] A single shared error-reporting function/module is used by both bridging paths rather than duplicated ad hoc handling.
- [ ] Error messages are actionable where possible (e.g. "xclip not found — is it installed and on PATH?" vs. a generic failure string).
- [ ] Manual test: force a failure (e.g. rename the backend binary temporarily, or run under an environment with no `DISPLAY`/`WAYLAND_DISPLAY`) and confirm the message appears.

## Technical notes
- `Event::SystemClipboardFailure` requires the `ReadApplicationState` permission and carries no further detail beyond "a copy failed somewhere in Zellij" — treat it as a signal, not a detailed error.
- `Event::RunCommandResult(Option<i32>, Vec<u8>, Vec<u8>, BTreeMap<String,String>)` provides exit code, stdout, and stderr for shell-out failures, which is more actionable than `SystemClipboardFailure` and should be preferred for user-facing detail when the shell-out path (issue 33) is in use.

## Out of scope
- Automatic retry logic on failure (may be a future enhancement).
- Telemetry/logging to external systems; this issue covers in-plugin UI surfacing only.

## Depends on
Primary path: bridge yanks through copy_to_clipboard; Shell-out backends for xclip, wl-copy, pbcopy and clip.exe
