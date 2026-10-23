---
title: Bounded local display cache for remote map and logs
author: Khayalsan
date: 2026-10-23
branch: feat/remote-display-cache
touches:
  - CHANGELOG.md
  - src-tauri/src/lib.rs
  - src/App.tsx
  - src/components/LogsPanel.tsx
  - src/components/PlayerMapPanel.tsx
  - src-tauri/src/display_cache.rs
  - src/components/DisplayCacheSettings.tsx
  - src/lib/displayCache.ts
---

## Decided
Keep remote map terrain, last map view, and recent Logs output in a bounded per-profile display cache under Tauri app data. Show cached data first, then refresh over the existing SSH reads. Cache writes are best effort and do not block remote reads. The App settings size and Clear action apply only to this new display cache.

## Tried and ruled out
Caching server status, access controls, editable files, or world settings as if current. Those values can change server actions and must come from fresh reads. Reading whole Anvil region files over SSH was already ruled out by the existing terrain design.

## Watch out
A cached map roster must not claim a player is currently online. Map tiles can use stored region timestamps for conditional refresh, but manual Refresh must force a read. Higher-resolution cached tiles can paint a lower-detail view while its own tiles are fetched. A missing or failed remote tile read should leave the cached image visible.
