---
title: Match forest map tone and version cached terrain colors
author: Khayalsan
date: 2026-10-23
branch: feat/remote-display-cache
touches:
  - CHANGELOG.md
  - src-tauri/src/display_cache.rs
  - src-tauri/src/terrain.rs
---

## Decided
Use Minecraft map-like plant and grass base colors for terrain tiles, with darker stepped height shading so dense forests retain the deeper contrast seen on in-game maps. Change the on-disk terrain tile namespace from terrain to terrain-v2 so old RGBA tiles cannot keep the previous tone when chunk timestamps are unchanged.

## Tried and ruled out
A CSS saturation filter would recolor water, stone, and markers together, obscuring the block-specific cause. Merely changing the backend palette without versioning the cache would leave previously saved tiles unchanged because conditional chunk reads can return unchanged.

## Watch out
This is still an approximate terrain renderer based on surface blocks and saved chunk data; it does not reproduce every Minecraft map color rule. Keep old terrain tiles inside the managed display-cache root so the cache size and Clear cache action still account for them until pruning or clearing.
