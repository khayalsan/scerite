---
title: Keep Overworld visible as the default map dimension
author: Khayalsan
date: 2026-10-23
branch: feat/remote-display-cache
touches:
  - CHANGELOG.md
  - src/components/PlayerMapPanel.tsx
---

## Decided
- The Map tab always opens with Overworld selected, including when no players are there.
- Include Overworld in the dimension selector independently of player locations.
- Restore a cached map view only when it is an Overworld view; cached Nether or End views must not override the opening dimension.

## Tried and ruled out
- Building the dimension selector solely from player positions can hide Overworld when it is empty.
- Restoring the cached dimension on mount can reopen the tab in Nether after the player returns to Overworld.

## Watch out
- Manual dimension switching still works while the tab stays open. Do not auto-switch the selection in response to player movement.
