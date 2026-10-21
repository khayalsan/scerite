---
title: Restore selected or latest world backup safely
author: Khayalsan
date: 2026-10-21
branch: feat/world-settings
touches:
  - CHANGELOG.md
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/world_management.rs
  - src/App.css
  - src/components/WorldsPanel.tsx
  - src/lib/serverWorlds.ts
---

## Decided
World backup restoration accepts only an app-data ZIP or preserved server folder belonging to the selected profile and world. A stopped server is required. Stage ZIP contents before swapping; move the current world to a new preserved folder first, and roll it back if installing the selected backup fails. The UI offers a selected backup and the latest per world, including when the current world folder is absent.

## Tried and ruled out
Do not pass a displayed absolute backup path directly into generic upload or arbitrary file operations without checking it against the managed backup location and generated filename. Do not use preserved folder modification time as the primary backup date: moving a world preserves its older mtime.

## Watch out
New preserved folders include an `-at-<unix seconds>-<uuid>` suffix so latest ordering uses backup creation time; older UUID-only preserved folders remain visible with mtime fallback. Restoring a preserved folder moves that backup out of the list, while a ZIP remains available.
