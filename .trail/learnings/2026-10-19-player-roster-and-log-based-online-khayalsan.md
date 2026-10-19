---
title: Player roster and log-based online status
author: Khayalsan
date: 2026-10-19
branch: feat/server-players-logs
touches:
  - CHANGELOG.md
  - src-tauri/Cargo.lock
  - src-tauri/Cargo.toml
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src/App.css
  - src/App.tsx
  - src-tauri/src/server_players.rs
  - src/components/PlayersPanel.tsx
  - src/lib/serverPlayers.ts
---

## Decided
- The Players tab reads cached names, access lists, and saved UUID filenames through one local or remote roster command. Remote reads share one SSH session and SFTP subsystem; the UI reads latest.log separately for a best-effort online estimate.
- Whitelist, operator, and ban changes go through the running server console after validating names or IPs. Player NBT and statistics are read only, inside the managed server root.
## Tried and ruled out
- Calling the generic server-file reader and directory lister for every roster file and candidate folder created too many remote round trips.
- RCON was not used: it would need a server port and password, and the user selected log-based online detection.
## Watch out
- latest.log can be truncated or miss prior join events. Label online state as estimated and offer Refresh. Saved player NBT can lag behind live state.
- Trail check may match broad words like server, file, or remote against unrelated older learnings; review actual code paths before treating those as conflicts.
