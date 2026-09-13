# Persisting the buffer ring

Set `persist "session"` on zclip's plugin config block (see
[`examples/zclip.kdl`](../examples/zclip.kdl)) to keep the paste-buffer ring
alive across plugin reloads. It defaults to `persist "off"`: persistence is
opt-in, for reasons covered in [Privacy](#privacy) below.

## Spike findings

Issue #15 started as a spike: there was no documented, confirmed persistence
API for Zellij plugins, and the issue's own notes treated `/data` and
`/cache` as an unverified guess. The table below is what running Zellij
v0.45.1's actual source down settled, not a guess.

| Question | Answer | Source |
| --- | --- | --- |
| Are `/data`/`/cache` writable from inside the plugin sandbox? | Yes. Both, plus `/host` and `/tmp`, are preopened read-write and their directories are created before the plugin starts. | `zellij-server/src/plugins/plugin_loader.rs:432-435,454` (preopens); `:48-56` (dir creation) |
| Where do they map to on the host? | `plugin_own_data_dir = ZELLIJ_SESSION_CACHE_DIR/<url-safe-location>/<plugin_id>-<client_id>`; `plugin_own_cache_dir = ZELLIJ_CACHE_DIR/<url-safe-location>/plugin_cache`. | `zellij-server/src/plugins/wasm_bridge.rs:125-134` |
| What are those two roots? | `ZELLIJ_CACHE_DIR` is `~/.cache/zellij`. `ZELLIJ_SESSION_CACHE_DIR` is that same root plus a fresh `Uuid::new_v4()` generated per **server process**. | `zellij-utils/src/consts.rs:99-102` |
| What happens to `/data` on a plugin reload? | `unload_plugin` delivers `BeforeClose` to subscribers, then unconditionally `remove_dir_all`s `plugin_own_data_dir`. | `zellij-server/src/plugins/wasm_bridge.rs:496-561` |
| Does any permission gate file IO under `/cache`? | No. Zellij's `PermissionType` checks only cover `host_run_plugin_command`; there is no gate on WASI file IO. | (absence, checked against the full `PermissionType` match) |

**Conclusion, stated bluntly because the issue's own notes guessed
otherwise: `/data` is useless for this feature.** A plugin reload is
implemented as unload-then-load, and `unload_plugin` deletes
`plugin_own_data_dir` unconditionally as part of that same code path — the
exact operation this feature exists to survive. It also sits under a
per-server UUID, so even if it survived a reload it could not survive
anything else either. `/cache` is the only one of the four mounts that
survives a reload, which is why the ring is written there.

The absence of a permission gate matters too: persistence adds no
permission prompt of its own. That is precisely why `persist` must default
to `off` and be opted into deliberately — see [Privacy](#privacy).

## What persists and for how long

Each Zellij session has its own buffer ring. It survives reloading the
plugin, closing and re-opening zclip's pane, and detaching and
re-attaching. It does not survive ending the session or restarting the
Zellij server — exactly like tmux's paste buffers, which live and die with
the tmux server. Buffers never leak between sessions.

Concretely: the ring is written to a file named `ring-<zellij_pid>.zclip`
in a `zclip/` subdirectory of the plugin's `/cache` mount, which on the
host resolves to `~/.cache/zellij/<url-safe-plugin-location>/plugin_cache/zclip/`.
One file per Zellij server process — `zellij_pid` identifies the server,
not the plugin instance — so two sessions never write to, or read, the
same file.

Saves are debounced: a dirty ring is written to disk at most once per
2-second window, plus an unconditional flush on `Event::BeforeClose`. The
practical cost of that debounce is what you'd expect: a hard kill (SIGKILL,
power loss) can lose up to roughly 2 seconds of the most recent yanks.
Unloading the plugin — a reload, or closing zclip's pane — does not,
because Zellij delivers `BeforeClose` to subscribers before it unloads a
plugin (verified in `unload_plugin`, cited above) and zclip flushes there.
Detaching loses nothing either, for the simpler reason that it unloads
nothing at all: the server, the plugin and the in-memory ring all keep
running.

Writes are atomic — a temp file is written and renamed into place in the
same directory — so a crash mid-write can never leave a half-written ring
file behind. A ring file that is nonetheless corrupt or unreadable (e.g.
hand-edited, or from an incompatible future version) yields an empty ring
and a warning, never a crash. Individual malformed *records* inside an
otherwise-valid file are skipped and counted rather than failing the whole
restore, so a partly damaged file still restores whatever it can.

Two caps bound what actually gets written: 1 MiB per buffer and 4 MiB per
file. Named buffers get first claim on the 4 MiB budget, matching the
ring's own in-memory eviction policy of never evicting named buffers ahead
of unnamed ones. A buffer that does not fit is skipped, never truncated —
half a shell script pasted back into a prompt is worse than a buffer that
simply did not survive.

Ring files from sessions that no longer exist are garbage-collected after
7 days. This exists because pids get recycled: without a GC window, a new
Zellij server that happens to reuse an old, no-longer-running server's pid
could otherwise inherit that dead session's ring on first load.

Deleting the last buffer in a ring deletes the ring file, rather than
leaving a header-only file behind.

## Privacy

This is the section that matters most, so read it before turning `persist`
on.

Yanked text is routinely credentials: a token copied out of a CI log, a
secret pulled from `kubectl get secret -o yaml`, a password out of `pass`.
tmux's paste buffers live in RAM and die with the tmux server; turning on
`persist "session"` in zclip instead writes that same text to
`~/.cache/zellij/.../plugin_cache/zclip/` in **plaintext**.

Two things make this worse than it sounds at first:

- `std::fs::set_permissions` is unsupported on `wasm32-wasip1`, so the
  plugin has no way to restrict the file's mode. It lands with whatever
  permissions the host's umask produces — typically `0644`, world-readable
  on a shared machine — and there is nothing zclip can do about that from
  inside the sandbox.
- There is no permission prompt for file IO (see the spike findings above),
  so nothing in Zellij's own UI would ever tell a user that their
  scrollback selections had started hitting disk.

That combination is why `persist` defaults to `off` and has to be set
deliberately, rather than being a quality-of-life default.

To erase everything zclip has ever persisted:

```sh
rm -rf ~/.cache/zellij/*/plugin_cache/zclip/
```

To verify that the default really writes nothing, run a full yank/paste
session with `persist` unset (or explicitly `off`) and then check:

```sh
find ~/.cache/zellij -name '*.zclip'
```

This should return no output.

## File format v1

The on-disk format is defined in `crates/zclip-core/src/persist.rs`
(`encode`/`decode`). It is deliberately not `serde`+JSON: `zclip-core` is
dependency-free by design (`CONTRIBUTING.md`), and a derived `Deserialize`
would rebuild the ring field by field, bypassing `enforce_limit`, name
uniqueness, the blank-text rule and id/seq monotonicity — restoring has to
go *through* the ring's own logic, not around it. What is left is a small,
line-oriented text format:

```text
zclip-ring\t1
<seq>\t<name-field>\t<escaped-text>
<seq>\t<name-field>\t<escaped-text>
...
```

- **Header.** The first line is always exactly `zclip-ring` TAB `1` — a
  literal format tag plus a version number.
- **Record layout.** Every subsequent non-empty line is one buffer:
  `seq`, `name-field`, and `escaped-text`, tab-separated. `seq` is written
  for ordering purposes but is treated as advisory on read: `decode` sorts
  records by `seq` descending and renumbers them to a fresh, dense range,
  so a corrupt or adversarial `seq` (duplicates, `u64::MAX`) can never
  cause an overflow or ambiguity downstream.
- **The name field.** `-` means the buffer is unnamed. `n` followed by an
  escaped name means the buffer is named, including a bare `n` — a named
  buffer whose name is the empty string, distinct from an unnamed buffer.
- **The four escapes.** Only `\`, LF, CR, and TAB are escaped, as `\\`,
  `\n`, `\r`, and `\t` respectively. Every other byte — including NUL, ESC,
  and multi-byte UTF-8 — passes through unchanged. This is the minimum
  needed to keep the format strictly line-oriented without mangling actual
  buffer content.
- **What is deliberately not persisted, and why:**
  - `BufferId` — an in-memory identity only; persisting it would just
    invite duplicate-id or `id >= next_id` corruption cases to guard
    against, for no benefit. A fresh id is assigned to every buffer on
    restore.
  - `limit` (the ring's `buffer_limit`) — the *live* configuration wins at
    load time, so lowering `buffer_limit` between reloads takes effect
    immediately rather than being overridden by whatever limit was in
    effect when the file was written.
  - `next_id` / `next_seq` — derived fresh on restore, strictly greater
    than anything just restored, exactly the way `push` grows them for a
    newly captured buffer.
- **Forward compatibility.** An unrecognised version in the header (i.e.
  anything other than `1`) discards the file outright — no attempt is made
  to read what it can. This is a cache, not a document format: there is no
  user-facing reason to carry forward a persisted ring across an
  incompatible format change, and refusing to guess is simpler and safer
  than a partial parse of an unknown layout.

## Manual test checklist

This is the "documented manual test steps" half of the issue's acceptance
criteria — the filesystem behavior it covers is not practical to exercise
in `zclip-core`'s hermetic, dependency-free test suite (see
[`docs/dev.md`](dev.md)), so it is checked by hand instead.

Set `persist "session"` in your `zclip.kdl` before starting.

All eight steps below were last executed end-to-end against Zellij 0.45.1
on 2026-09-13 and passed. That run also confirmed empirically what the
spike above establishes from source: the ring file appeared at
`<cache>/zellij/file:<plugin-path>/plugin_cache/zclip/ring-<pid>.zclip`,
and its contents matched the v1 format byte for byte.

1. **Reload survives.** Yank a few buffers, then run
   `zellij action start-or-reload-plugin file:$PWD/target/wasm32-wasip1/debug/zclip.wasm`.
   Expected: the buffer list shows the same buffers after the reload.
2. **Pane close/reopen survives.** Close zclip's pane (not the session),
   then reopen it (e.g. `Alt b`). Expected: buffers are unchanged.
3. **Detach/attach survives.** `zellij detach`, then `zellij attach` back
   into the same session. Expected: buffers are unchanged.
4. **Session end does *not* survive.** Note the current buffers, then
   `zellij kill-session`, and start a fresh session with the same config.
   Expected: the ring is empty. This is the documented semantics, not a
   bug — see [What persists and for how long](#what-persists-and-for-how-long).
5. **Sessions do not share buffers.** Start two concurrent Zellij sessions,
   yank different text into each. Expected: `ls ~/.cache/zellij/*/plugin_cache/zclip/`
   shows one `ring-<pid>.zclip` file per session, and neither session's
   buffer list shows the other's buffers.
6. **Corrupt file fails safe.** Overwrite the ring file
   (`printf 'garbage' > <path-to-ring-file>`) and then reload the plugin
   *without yanking anything in between* — a yank would mark the ring dirty
   and the `BeforeClose` flush would helpfully overwrite your garbage with
   a valid file before the reload ever read it. Expected: an empty ring, a
   warning in the status line, and no crashed pane.
7. **Unwritable directory fails safe.** `chmod a-w` the `zclip/`
   persistence directory, then trigger a save (e.g. yank something).
   Expected: a warning in the status line, and the plugin otherwise stays
   fully usable — yank/paste/copy-mode all keep working in-memory.
8. **Privacy check.** With `persist` unset (the default), do a full
   yank/paste session, then run `find ~/.cache/zellij -name '*.zclip'`.
   Expected: no output.

## Known limitations

- Two zclip instances running inside one Zellij session share a single
  ring file. Whichever saves last wins; the loser's ring is fully replaced,
  not merged or corrupted, because the write itself is atomic — there is
  no partial-overwrite hazard, only a lost update.
- A hard kill (SIGKILL, power loss) can lose up to roughly 2 seconds of the
  most recent yanks, the width of the debounce window. A clean shutdown
  (detach, session end, plugin reload) never loses anything, because it
  goes through `BeforeClose` first.
- Buffers that do not fit the per-buffer or per-file cap are silently
  skipped on save. The only visible signal is the skipped count folded
  into the status line — there is no separate log or per-buffer notice.
- Pid recycling within the 7-day GC window is a real, if narrow, edge case:
  if a Zellij server exits, its ring file outlives it for up to 7 days, and
  a new server that happens to reuse the same pid within that window would
  load it as its own. This requires the same user on the same machine, and
  is bounded by the GC window; it is not a cross-user or cross-machine
  leak.
