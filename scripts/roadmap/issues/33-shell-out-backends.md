---
title: Shell-out backends for xclip, wl-copy, pbcopy and clip.exe
milestone: M3 - System Clipboard Bridge
labels: story,area:clipboard,priority:p1
---

## Context
When zclip wants direct control over which clipboard command runs (independent of Zellij's own `copy_command` config, or as an explicit user override from issue 36), it must shell out itself via `run_command`. All four target backends (xclip, wl-copy, pbcopy, clip.exe) read the text to copy from stdin — but Zellij's `run_command` shim does not support piping stdin to the spawned process. This issue exists specifically to solve that mismatch safely.

## Acceptance criteria
- [ ] Confirm and document that `run_command` / `run_command_with_env_variables_and_cwd` provide no stdin-piping mechanism, so the naive `run_command(&["xclip", "-selection", "clipboard"], ...)` approach cannot deliver text to the child process.
- [ ] Implement a workaround that gets text into the backend's stdin without passing it as a bare argv element — e.g. invoking `sh -c` with a shell here-string/here-doc, or writing text to a temp file and redirecting it as stdin within the `sh -c` invocation.
- [ ] Explicitly evaluate and document the security implication of any approach that puts clipboard text into a process's argv (visible via `ps`/`/proc/<pid>/cmdline` on other users' sessions on shared systems) — prefer an approach that avoids this exposure, and justify the final choice in the PR description.
- [ ] Temp-file-based approaches (if used) create files with restrictive permissions and clean them up reliably, including on command failure.
- [ ] `Event::RunCommandResult(exit_code, stdout, stderr, context)` is consumed to detect non-zero exit codes and stderr output, wiring failures into issue 34.
- [ ] The backend selected in issue 32 (or overridden per issue 36) determines which concrete command line is constructed for xclip, wl-copy, pbcopy, or clip.exe.
- [ ] Manual verification steps are documented for at least xclip (X11) and wl-copy (Wayland); pbcopy/clip.exe verification may be documented as "needs a macOS/WSL tester" if unavailable in CI.

## Technical notes
- `run_command(cmd: &[&str], context: BTreeMap<String, String>)` and `run_command_with_env_variables_and_cwd(...)` require the `RunCommands` permission. https://docs.rs/zellij-tile/latest/zellij_tile/shim/fn.run_command.html
- Results arrive asynchronously as `Event::RunCommandResult(Option<i32>, Vec<u8>, Vec<u8>, BTreeMap<String, String>)`; the `context` map is the mechanism for correlating a result back to the request that triggered it.
- Known xclip/wl-copy/pbcopy invocations: `xclip -selection clipboard`, `wl-copy`, `pbcopy` — all read from stdin, per Zellij's own `copy_command` documentation. https://zellij.dev/documentation/options.html
- Because `run_command` has no native stdin support, this is the central engineering problem of this issue — do not treat it as a minor detail.

## Out of scope
- Backend detection logic itself (issue 32).
- The `copy_to_clipboard` primary path, which sidesteps this stdin problem entirely by delegating to Zellij (issue 31).

## Depends on
Detect the platform clipboard backend
