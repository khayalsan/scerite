---
title: Safe world settings and ZIP operations
author: Khayalsan
date: 2026-10-21
branch: feat/world-settings
touches:
  - CHANGELOG.md
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src/App.css
  - src/App.tsx
  - src/components/WorldsPanel.tsx
  - src-tauri/src/world_management.rs
  - src/lib/serverWorlds.ts
---

## Decided
World settings live in the existing Worlds tab. Read the actual seed and saved gamerules from level.dat; send gamerule changes to a running active world. Keep backup, ZIP download, ZIP upload, and regeneration limited to stopped servers. ZIP exports are per selected world folder; an upload preserves the replaced folder under .scerite-world-backups. Regeneration also preserves matching Paper-style Nether and End folders before Minecraft creates a fresh world on next start.

## Tried and ruled out
Do not mutate level.dat for offline gamerules. Do not extract a ZIP over the active world. Validate ZIP paths and size first, extract into a separate staging folder, then move the old world aside before replacing it.

## Watch out
Remote transfers stream through SFTP and can take time for large worlds. Backup ZIPs are saved in Scerite app data; regeneration/upload preservation folders are on the server. The UI's gamerule values come from the last saved level.dat and a sent console command is not proof that Minecraft accepted it. A server start while an archive is in progress can still make a backup inconsistent; the operation checks stopped status before starting, but there is no cross-command world lock yet.
