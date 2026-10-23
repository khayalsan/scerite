---
title: Preserve map detail across zoom level changes
author: Khayalsan
date: 2026-10-23
branch: feat/remote-display-cache
touches:
  - CHANGELOG.md
  - src/components/PlayerMapPanel.tsx
  - src/lib/terrainTiles.ts
---

## Decided
Keep separate bounded in-memory tile LRUs for resolution 1, 4, and 16. A wide low-detail view must not evict high-detail tiles from the same area. When another resolution returns a newer chunk timestamp, keep the old image as a temporary fallback but mark it stale so that returning to that resolution fetches an update immediately.

## Tried and ruled out
One shared 512-entry LRU across all detail levels caused zoom-out tiles to displace high-detail tiles. Deleting every other resolution on a changed timestamp recreated the temporary blur during refresh.

## Watch out
A forced refresh should bypass conditional remote reads, not discard still-useful high-detail canvases when the timestamp is unchanged. Keep the per-resolution limits bounded and preserve lower-resolution fallback while a fresh read is in flight.
