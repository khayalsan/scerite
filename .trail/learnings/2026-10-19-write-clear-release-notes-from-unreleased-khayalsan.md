---
title: Write clear release notes from Unreleased
author: Khayalsan
date: 2026-10-19
branch: feat/release-changelog
touches:
  - CHANGELOG.md scripts/prepare-release.mjs .github/workflows/release.yml
---

## Decided
Write Unreleased notes in plain language about user-visible behavior. Keep the existing release history and let prepare-release move Unreleased notes into the next version when a pull request merges.

## Tried and ruled out
A tmux-specific "missing pane" bullet was too technical for release notes; describe the misleading SSH error users actually saw. Do not call the branch's Unreleased notes 0.3.1 while the v0.3.1 tag already exists.

## Watch out
Check live tags before naming a pending release. The workflow picks the next version from tags, so a merge with v0.3.1 present creates v0.3.2. Trail check flags changelog wording against older implementation decisions; inspect actual code and note the mismatch in the commit or PR.
