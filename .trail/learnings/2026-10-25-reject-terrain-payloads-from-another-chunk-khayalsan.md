---
title: Reject terrain payloads from another chunk coordinate
author: Khayalsan
date: 2026-10-25
branch: fix/map-tile-coordinate-mismatch
touches:
  - CHANGELOG.md
  - src-tauri/src/terrain.rs
---

## Decided
The screenshot's wrong X 192-208, Z 112-128 tile sampled exactly matched the existing cached 16x16 RGBA image for chunk (16,24), with zero per-channel squared error; the saved (12,7) tile differs. The decoder previously labeled any NBT payload with the requested (x,z) without validating the payload's xPos/zPos. Reject mismatched coordinates before display or cache, and keep the earlier fresh-read-after-hydration safeguard.
## Tried and ruled out
A same-chunk age or color difference does not explain a pixel-perfect match to (16,24). Cache paths and frontend draw coordinates are keyed consistently. The original bad (12,7) pixels were overwritten, so neither a transient live region read nor an earlier cache write can be proven as the historical trigger.
## Watch out
The coordinate check requires xPos/zPos in the chunk NBT and returns an error tile on mismatch. The targeted synthetic-payload test reproduces the old acceptance path and verifies rejection. Preserve cached tiles on read error per map behavior, but surface the map warning.
