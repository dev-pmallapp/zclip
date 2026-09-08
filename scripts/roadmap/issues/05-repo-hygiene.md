---
title: Repo hygiene: README skeleton, CONTRIBUTING, issue and PR templates
milestone: M0 - Scaffold & CI
labels: task,area:docs,priority:p1
---

## Context
The repository is currently empty apart from a LICENSE file. Baseline
documentation and contribution scaffolding are needed before external
contributors (or future maintainers) can orient themselves, install the
plugin, or open well-formed issues and pull requests.

## Acceptance criteria
- [ ] README.md describes what zclip is, its status, install instructions, and a KDL config snippet
- [ ] README includes a CI status badge and links to the dev-loop docs
- [ ] CONTRIBUTING.md documents the build/test/lint commands and PR expectations
- [ ] `.github/ISSUE_TEMPLATE/` has at least a bug-report and feature-request template
- [ ] `.github/PULL_REQUEST_TEMPLATE.md` exists with a checklist (tests, fmt, clippy)
- [ ] A CHANGELOG.md (or "Keep a Changelog" stub) is initialized

## Technical notes
- Keep README config examples aligned with actual `load()`/KDL config keys as they are added
  (e.g. eventual `buffer_limit` option) so docs do not drift from code
- No external APIs involved; this is documentation-only work

## Out of scope
- Full user-facing documentation site
- API reference generation (rustdoc publishing)

## Depends on
Bootstrap the Rust WASM plugin crate
