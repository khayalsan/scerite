---
title: Map-like saved terrain and console TPS probing
author: Khayalsan
date: 2026-10-22
branch: feat/player-map
touches:
  - CHANGELOG.md
  - src-tauri/src/terrain.rs
  - src/App.css
  - src/components/ServerOverviewPanel.tsx
---

## Decided
- The Server tab should probe the console `tps` command even when profile software is not Paper-compatible; an installed mod may provide it. Continue 30-second polling only after a response for those loaders.
- Parse wrapped tmux responses and use the last TPS reading. Saved terrain uses brighter map-like block colors, local north/south slope shading, and water depth shading without changing tile bounds or transfer budget.
## Tried and ruled out
- Showing `Unavailable` solely from software metadata when `tps` works in the Console.
- Using absolute world height as the only terrain shade; it flattens hills and coastlines.
## Watch out
- Exact TPS output formats vary; a console response not matching the parser should be inspected before adding a format. The terrain colors approximate Minecraft maps and use saved blocks, without biome tinting.
