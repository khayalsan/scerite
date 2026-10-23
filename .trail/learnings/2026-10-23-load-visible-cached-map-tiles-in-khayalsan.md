---
title: Load visible cached map tiles in one local pass
author: Khayalsan
date: 2026-10-23
branch: feat/remote-display-cache
touches:
  - CHANGELOG.md
  - src-tauri/src/display_cache.rs
  - src/components/PlayerMapPanel.tsx
---

## Decided
Read up to the 512 visible cached map tiles in one local app-data request before starting terrain refresh. Run the disk scan on a blocking worker so it does not hold the Tauri UI thread. Keep the existing 16-chunk SSH batches, viewport cap, and read budget.

## Tried and ruled out
Using the 16-chunk remote batch size for local cache reads. It delayed already saved terrain behind repeated local calls and the remote pacing pause.

## Watch out
Validate cache request size separately from the remote terrain request limit. A pan or zoom can invalidate a pending local read; check the terrain generation before drawing the result. Manual Refresh should skip cache hydration and force the remote read.
