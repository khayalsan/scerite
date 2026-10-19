---
title: Publish changelog notes from merge releases
author: Khayalsan
date: 2026-10-19
branch: feat/release-changelog
touches:
  - ".github/workflows/release.yml"
  - scripts/prepare-release.mjs
  - CHANGELOG.md
  - scripts/release-notes.mjs
---

## Decided
A merged pull request to main starts a desktop release. Keep the changelog workflow and the pending package-size/short-search fixes together in one branch so one merge creates one release. Maintain notes under Unreleased; prepare-release moves them into the chosen version, and publish uses that version's section as the GitHub release body.

## Tried and ruled out
Merging the pending fix branch before the changelog workflow would publish a release without maintained notes. Publishing from an empty Unreleased section would also produce a release without useful notes, so preparation fails early.

## Watch out
The release workflow checks out the merge commit, writes version files and CHANGELOG.md, commits them to main, then tags that commit. The publish job must check out that tag before extracting notes.
