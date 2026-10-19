---
title: Prepare a single-branch desktop release
author: Khayalsan
date: 2026-10-19
branch: feat/server-players-logs
touches:
  - package.json package-lock.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json
---

## Decided
- For a single feature branch, prepare the requested release version on that branch and open its PR to main; the release workflow runs when the PR merges into main.
- Keep package.json, package-lock.json root entries, Cargo.toml, the scerite Cargo.lock entry, and tauri.conf.json on the same version.
- Leave the human-readable notes under Unreleased; prepare-release.mjs moves them into the versioned section when the release workflow runs.

## Tried and ruled out
- package-lock.json was still 0.3.1 while the other version sources were 0.3.2, so updating only package.json would leave inconsistent release metadata.
- A pre branch was unnecessary for this single feature branch and did not exist on the remote.

## Watch out
- The release workflow is triggered by a merged PR into main, not by opening the PR or pushing a commit.
- The version must exceed the highest vX.Y.Z tag, or prepare-release.mjs may choose a different patch version.
