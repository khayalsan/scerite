---
title: Auto backup on observed server stop with isolated retention
author: Khayalsan
date: 2026-10-21
branch: feat/world-settings
touches:
  - CHANGELOG.md
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/server_profiles/mod.rs
  - src-tauri/src/server_profiles/store.rs
  - src-tauri/src/world_management.rs
  - src/App.css
  - src/App.tsx
  - src/components/WorldsPanel.tsx
  - src/lib/serverWorlds.ts
---

## Decided
Auto backup is a per-server setting in the existing profile store, disabled by default with keep=3. While the desktop app is open, its existing status polling detects a known running/stopping to stopped transition and invokes a stopped-server backup of the active world. Automatic ZIPs live in world-backups/<profile>/auto; manual ZIPs stay in the parent folder. Retention runs after a successful automatic ZIP and only removes older automatic ZIPs for that world. The new ZIP wins timestamp ties. Manual ZIPs have an explicit Delete action.

## Tried and ruled out
Do not prune the shared backup folder: it contains manual ZIPs that must remain. Do not rely on a stop command returning before Minecraft has saved and exited; status polling waits for stopped. A server-side daemon or periodic remote polling while Scerite is closed was not added.

## Watch out
Automatic backup requires Scerite to observe the stop while it is open. The backend rechecks stopped state, validates server.properties and the world folder, and reports failures through the server action banner. Existing backup listing and restoration include both manual and automatic ZIP paths.
