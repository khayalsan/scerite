---
title: Disk cached map tiles need one fresh remote read
author: Khayalsan
date: 2026-10-25
branch: fix/map-tile-coordinate-mismatch
touches:
  - CHANGELOG.md
  - src/components/PlayerMapPanel.tsx
  - src/lib/terrainTiles.ts
---

## Decided
A tile hydrated from local display cache can be drawn immediately but must not use its saved region timestamp as proof it matches the current remote chunk. Mark disk-loaded tiles unverified; request their pixels once from the server, then allow timestamp-based unchanged responses for tiles fetched in this map session.
## Tried and ruled out
The reported X 192-208, Z 112-128 tile maps to chunk 12,7 with no index overflow. Manually refreshing corrected it, pointing to stale cache rather than decoding the wrong region entry. Clearing all map cache or delaying cached display would lose the fast initial map.
## Watch out
Propagate the unverified flag through derived low-resolution tiles and keep the existing bounded remote batches. A failed read retains cached pixels and the retry cooldown, rather than blanking the map.
