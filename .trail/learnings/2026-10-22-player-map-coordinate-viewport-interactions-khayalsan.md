---
title: Player map coordinate viewport interactions
author: Khayalsan
date: 2026-10-22
branch: feat/player-map
touches:
  - CHANGELOG.md
  - src/App.css
  - src/components/PlayerMapPanel.tsx
---

## Decided
- The player map keeps a world-coordinate viewport (`centerX`, `centerZ`, blocks per pixel) after the user pans or zooms; Fit returns to bounds derived from current player positions.
- Pan by pointer drag or arrow keys, zoom around the cursor with the wheel or from the center with buttons, and center a selected player from the side list.
- Draw grid lines from world coordinates so labels and the origin move correctly with the viewport. Keep marker images at a fixed readable size while the map scale changes.

## Tried and ruled out
- Keeping the previous percentage positions and clamping markers to the map edges: that would make zoom and pan misleading because off-screen players would still appear at an edge.

## Watch out
- A `wheel` listener needs `{ passive: false }` to prevent the surrounding panel from scrolling during map zoom.
- The map has no terrain imagery yet; it is a coordinate grid. Keep the In Progress badge until that product decision changes.
- `trail check` can flag broad lexical matches in existing dev changelog and command-registration lines. These matches are unrelated to viewport interaction.
