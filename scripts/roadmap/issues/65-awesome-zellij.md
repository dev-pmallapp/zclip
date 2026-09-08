---
title: Submit zclip to awesome-zellij
milestone: M6 - v0.1.0 Release
labels: task,area:docs,priority:p2
---

## Context
With no official plugin registry, `awesome-zellij` is the de facto discovery point for Zellij plugins. Getting zclip listed there after v0.1.0 ships is the main lever for organic adoption beyond people who already know about the project.

## Acceptance criteria
- [ ] The `awesome-zellij` repository's contribution guidelines have been read and followed
- [ ] A PR is opened against `awesome-zellij` adding zclip with a short description, matching the README's summary
- [ ] The PR links to the zclip repo and, if required by the list's format, to the latest release
- [ ] The PR is opened only after v0.1.0 is tagged and the release workflow (#62) has produced a working artifact
- [ ] The PR link is tracked in this issue until merged (or closed with reviewer feedback addressed)

## Technical notes
- Community index: https://github.com/zellij-org/awesome-zellij
- Follow that repo's existing entry format exactly (list formatting, category placement) rather than inventing a new style.

## Out of scope
- Maintaining the awesome-zellij entry long-term (a one-time submission for v0.1.0; update in a future issue if the format changes)
- Submitting to any other third-party curated lists

## Depends on
Automated release workflow publishing zclip.wasm
