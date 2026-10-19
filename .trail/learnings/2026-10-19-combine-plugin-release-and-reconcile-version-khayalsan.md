---
title: Combine plugin release and reconcile version tags
author: Khayalsan
date: 2026-10-19
branch: feat/plugin-release-0.3.1
touches:
  - package.json
  - README.md
---

## Decided
- Release v0.3.1 from one combined PR containing plugin updates/removal, the SSH channel fix, plugin search, the PR-merge release trigger, and aligned 0.3.1 version files.
- With remote v0.3.1 and v0.3.2 tags and the v0.3.1 draft removed, prepare-release.mjs selects v0.3.1 when package.json is 0.3.1 and latest tag is v0.3.0.

## Tried and ruled out
- Merging the individual fixes or release-trigger branch separately would start a release per merged PR. Keep this batch in one PR.
- Trail check on the combined branch reports lexical conflicts on README.md and modrinth.rs. Review of the cited spans found no reintroduction of the rejected behavior: compatibility is checked from Modrinth metadata, and plugin update installs and verifies the replacement before removing the old JAR.

## Watch out
- The merge event, not a direct main push, starts the release workflow. Do not merge the batch until the release is intended.
- Confirm the published GitHub release and updater artifacts after the workflow completes. The five version files must all remain 0.3.1.
