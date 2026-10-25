---
title: Wrong map chunk was displayed from cache
author: Khayalsan
date: 2026-10-25
branch: fix/map-tile-coordinate-mismatch
---

## Decided
The reported map glitch was terrain from a different chunk displayed at X 192-208, Z 112-128, not an older image of that same chunk. Manual Refresh replaces it. The disk-cache verification change in 8857066 will replace a wrongly associated cached tile during automatic remote refresh, regardless of why its pixels were wrong.
## Tried and ruled out
Calling the symptom merely a stale same-chunk image misstates the user's observation. Current cache paths and renderer use profile, dimension, resolution, x, and z consistently; no coordinate overflow was found for chunk 12,7.
## Watch out
A cached-first map can still show wrongly associated pixels briefly before its fresh read completes. The exact original source of the bad cached pixels remains unproven without the affected cache file and fresh region data; do not claim a specific coordinate-pairing bug was established.
