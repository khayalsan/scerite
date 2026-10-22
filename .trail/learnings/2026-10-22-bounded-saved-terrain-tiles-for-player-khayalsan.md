---
title: Bounded saved terrain tiles for player map
author: Khayalsan
date: 2026-10-22
branch: feat/player-map
touches:
  - CHANGELOG.md
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src/App.css
  - src/components/PlayerMapPanel.tsx
  - src-tauri/src/terrain.rs
  - src/lib/terrainTiles.ts
---

## Decided
- Render saved terrain from Anvil `.mca` region files through existing local filesystem and SSH/SFTP paths. Read only requested region headers and chunk byte ranges; no server plugin, custom agent, or management port.
- A chunk is shown only when its region entry exists and its NBT status is full. Decode modern section palettes and packed block states, sample top blocks into 16x16, 4x4, or 1x1 color tiles based on map zoom.
- Compare each region-header chunk timestamp with the cached tile timestamp on a 45-second poll while the Map tab is mounted. Manual Refresh bypasses the timestamp check. Decode/read failures preserve existing cached tiles; new ungenerated areas remain blank.
- Keep requests serial and bounded: 16 chunks per backend call, at most 512 chunks near players and map center per viewport, a 32 MiB compressed-read budget per viewport work, 8 MiB decompression cap per chunk, and a 512-entry memory tile cache. Stop scheduling work on unmount.

## Tried and ruled out
- Reading whole region files over SSH. Region files can be large and would transfer data far outside the visible viewport.
- Showing full-resolution block tiles at every zoom. Zoomed-out terrain uses coarser saved tiles or derives them from a finer cached tile.

## Watch out
- Supported region payload compression is gzip, zlib, or uncompressed. Unsupported/external chunks produce a read error and retain any cached tile.
- Saved chunks can change while a server writes them. A failed decode is recoverable on the next poll or manual Refresh.
- `trail check` reports broad lexical matches in older dev changelog and command registrations; this change does not switch online-player status to file polling or restore any world ZIP over active files.
