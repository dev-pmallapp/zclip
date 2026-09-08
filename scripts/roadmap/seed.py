#!/usr/bin/env python3
"""Idempotently seed zclip's GitHub labels, milestones and issues.

Usage:
    python3 scripts/roadmap/seed.py [--repo owner/name] [--dry-run]

Requires the `gh` CLI, authenticated with a token that has
"Issues: Read and write" on the target repository.

Re-running is safe: labels are upserted, and milestones/issues are matched
by title and skipped if they already exist.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent

LABELS = [
    ("epic", "6f42c1", "Large body of work tracked by child issues"),
    ("story", "0e8a16", "User-facing story / feature slice"),
    ("task", "c5def5", "Implementation task"),
    ("area:core", "1d76db", "Yank-buffer engine and state"),
    ("area:clipboard", "fbca04", "System clipboard bridge (xclip/wl-copy/pbcopy/clip.exe)"),
    ("area:ui", "d4c5f9", "Plugin rendering and UX"),
    ("area:config", "bfd4f2", "Config, keybindings, plugin aliases"),
    ("area:ci", "ededed", "Build, CI/CD and release automation"),
    ("area:docs", "0075ca", "Documentation"),
    ("priority:p0", "b60205", "Must have for v0.1.0"),
    ("priority:p1", "d93f0b", "Should have"),
    ("priority:p2", "fef2c0", "Nice to have"),
]

MILESTONES = [
    (
        "M0 - Scaffold & CI",
        "Stand up the Rust/WASM plugin crate, the permission bootstrap, the "
        "hot-reload dev loop and a green CI pipeline. Exit criteria: a no-op "
        "zclip.wasm loads in Zellij, requests its permissions, and CI enforces "
        "fmt + clippy + test + wasm build on every PR.",
    ),
    (
        "M1 - Core Yank Buffer Engine",
        "The tmux-like paste-buffer ring: bounded history, named buffers, yank "
        "and paste primitives, and persistence across plugin reloads. Exit "
        "criteria: text can be yanked into the ring and pasted into a pane, "
        "with ring semantics covered by unit tests.",
    ),
    (
        "M2 - Copy Mode & Selection",
        "A tmux-style copy mode driven from pane scrollback: key interception, "
        "cursor movement, char/line/block selection and in-scrollback search. "
        "Exit criteria: a user can enter copy mode, navigate scrollback, select "
        "text and yank it without touching the mouse.",
    ),
    (
        "M3 - System Clipboard Bridge",
        "Optional bridging of the yank buffer to the OS clipboard via Zellij's "
        "copy_to_clipboard API, with shell-out backends (xclip, wl-copy, pbcopy, "
        "clip.exe) and OSC 52 fallback. Exit criteria: yanked text reaches the "
        "system clipboard on X11, Wayland, macOS and WSL, with clear failure "
        "reporting.",
    ),
    (
        "M4 - Buffer Browser UI",
        "The interactive buffer picker: list, preview, filter and act on stored "
        "buffers, themed to the user's Zellij palette. Exit criteria: the plugin "
        "pane renders a usable, themed, responsive buffer browser.",
    ),
    (
        "M5 - Config, Keybindings & Pipes",
        "Everything that makes zclip scriptable and configurable: plugin "
        "configuration, `zellij pipe` commands, plugin-to-plugin messaging, "
        "headless mode and shipped example KDL. Exit criteria: zclip is fully "
        "driveable from config and the CLI.",
    ),
    (
        "M6 - v0.1.0 Release",
        "Ship it: docs, automated release artifacts, a compatibility matrix, "
        "cross-platform QA and ecosystem listing. Exit criteria: a tagged "
        "v0.1.0 GitHub release carrying a downloadable zclip.wasm.",
    ),
]


def gh(args: list[str], check: bool = True) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["gh", *args], capture_output=True, text=True, check=False
    ) if not check else _checked(["gh", *args])


def _checked(cmd: list[str]) -> subprocess.CompletedProcess:
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        sys.exit(f"FAILED: {' '.join(cmd)}\n{proc.stderr.strip()}")
    return proc


def parse_issue(path: pathlib.Path) -> dict:
    raw = path.read_text(encoding="utf-8")
    if not raw.startswith("---\n"):
        sys.exit(f"{path.name}: missing '---' front matter header")
    _, front, body = raw.split("---\n", 2)
    meta: dict[str, str] = {}
    for line in front.strip().splitlines():
        if not line.strip():
            continue
        key, _, value = line.partition(":")
        meta[key.strip()] = value.strip()
    for required in ("title", "milestone", "labels"):
        if required not in meta:
            sys.exit(f"{path.name}: front matter is missing '{required}'")
    return {
        "title": meta["title"],
        "milestone": meta["milestone"],
        "labels": [x.strip() for x in meta["labels"].split(",") if x.strip()],
        "body": body.strip() + "\n",
        "source": path.name,
    }


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default=None, help="owner/name (default: current repo)")
    ap.add_argument("--dry-run", action="store_true", help="print actions only")
    args = ap.parse_args()

    repo = args.repo or _checked(
        ["gh", "repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"]
    ).stdout.strip()
    dry = args.dry_run
    print(f"repo: {repo}{'  (dry run)' if dry else ''}\n")

    # --- labels -----------------------------------------------------------
    for name, color, desc in LABELS:
        if dry:
            print(f"  label   ~ {name}")
            continue
        proc = subprocess.run(
            ["gh", "label", "create", name, "--repo", repo, "--color", color,
             "--description", desc, "--force"],
            capture_output=True, text=True,
        )
        status = "ok" if proc.returncode == 0 else f"ERR {proc.stderr.strip()}"
        print(f"  label   {status:<4} {name}")

    # --- milestones -------------------------------------------------------
    existing = {}
    if not dry:
        out = _checked(
            ["gh", "api", "--paginate", f"repos/{repo}/milestones?state=all"]
        ).stdout
        existing = {m["title"]: m["number"] for m in json.loads(out)}

    for title, desc in MILESTONES:
        if dry:
            print(f"  milestone ~ {title}")
            continue
        if title in existing:
            print(f"  milestone skip {title} (#{existing[title]})")
            continue
        proc = subprocess.run(
            ["gh", "api", "-X", "POST", f"repos/{repo}/milestones",
             "-f", f"title={title}", "-f", f"description={desc}"],
            capture_output=True, text=True,
        )
        if proc.returncode != 0:
            sys.exit(
                "Could not create milestones.\n"
                "Your token likely lacks 'Issues: Read and write' on this repo.\n"
                f"{proc.stderr.strip()}"
            )
        existing[title] = json.loads(proc.stdout)["number"]
        print(f"  milestone ok   {title}")

    # --- issues -----------------------------------------------------------
    files = sorted(HERE.glob("issues/*.md"))
    if not files:
        sys.exit("No issue files found in scripts/roadmap/issues/")

    open_titles = set()
    if not dry:
        out = _checked(
            ["gh", "issue", "list", "--repo", repo, "--state", "all",
             "--limit", "500", "--json", "title"]
        ).stdout
        open_titles = {i["title"] for i in json.loads(out)}

    known = {t for t, _ in MILESTONES}
    print()
    for path in files:
        issue = parse_issue(path)
        if issue["milestone"] not in known:
            sys.exit(f"{path.name}: unknown milestone {issue['milestone']!r}")
        if dry:
            print(f"  issue   ~ [{issue['milestone']}] {issue['title']}")
            continue
        if issue["title"] in open_titles:
            print(f"  issue   skip {issue['title']}")
            continue
        cmd = ["gh", "issue", "create", "--repo", repo,
               "--title", issue["title"], "--body", issue["body"],
               "--milestone", issue["milestone"]]
        for label in issue["labels"]:
            cmd += ["--label", label]
        proc = subprocess.run(cmd, capture_output=True, text=True)
        if proc.returncode != 0:
            print(f"  issue   ERR  {issue['title']}: {proc.stderr.strip()}")
            continue
        print(f"  issue   ok   {proc.stdout.strip()}  {issue['title']}")

    print("\ndone.")


if __name__ == "__main__":
    main()
