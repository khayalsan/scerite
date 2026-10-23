---
title: Retire unreliable TPS overview polling
author: Khayalsan
date: 2026-10-23
branch: dev
touches:
  - CHANGELOG.md
  - src/components/ServerOverviewPanel.tsx
---

## Decided
- Remove the Server tab TPS row and its automatic console command polling. The user observed it remains unavailable even on Paper and is unsupported across other software.
- Keep the console available for users to issue software-specific commands manually.
## Tried and ruled out
- Extending the console parser and increasing polling delays did not make the overview readout reliable in the user's development build.
## Watch out
- When removing an unreleased feature, remove its Added and Fixed changelog claims too. Feature branches based on dev must incorporate this removal before their PRs so they do not restore the feature.
