---
title: Player map grid follows Minecraft chunks
author: Khayalsan
date: 2026-10-22
branch: feat/player-map
touches:
  - CHANGELOG.md
  - src/components/PlayerMapPanel.tsx
---

## Decided
- The player map grid is tied to Minecraft chunk boundaries: one chunk is 16x16 blocks, so line coordinates must be multiples of 16 on both X and Z.
- At wider zoom levels, show 2, 4, 8, etc. chunks per grid interval to avoid visual clutter; never switch to decimal 5/10/20 block spacing.
- Label the current interval in chunks and blocks.

## Tried and ruled out
- Generic decimal "nice number" grid steps. They can place lines at 5, 10, or 20 blocks and misrepresent chunk boundaries.

## Watch out
- Negative coordinates still need lines on exact multiples of 16; the existing ceiling-from-viewport logic preserves this when the step is a chunk multiple.
