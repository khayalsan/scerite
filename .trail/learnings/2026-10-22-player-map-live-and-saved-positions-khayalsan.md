---
title: Player map live and saved positions
author: Khayalsan
date: 2026-10-22
branch: feat/player-map
touches:
  - CHANGELOG.md
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/server_players.rs
  - src/App.css
  - src/App.tsx
  - src/lib/onlinePlayers.ts
  - src/lib/serverPlayers.ts
  - src/components/PlayerMapPanel.tsx
  - src/lib/playerLocations.ts
---

## Decided
- The Map tab uses the existing staged player roster so cached names and offline players can appear before the full roster read finishes.
- Read saved Pos and Dimension NBT in one batch operation per server, using one local root lookup or one remote SSH/SFTP session instead of opening a connection per player.
- While the Map tab is open and the server is running, query online players through the existing console session with `execute as @a run data get entity @s Pos` and `Dimension`; poll every 30 seconds. Label live and saved positions distinctly, and grey out offline heads.

## Tried and ruled out
- Deriving live positions from player data files: those files are saved intermittently and can lag behind a moving player.
- Adding a custom server-side agent or extra network port: the existing console session can answer the requested position query.

## Watch out
- The console response parser assumes Minecraft's English `Name has the following entity data:` output and may need adjustment for custom servers. The UI falls back to saved positions when live reads fail.
- Saved player data represents the last persisted position; it may not yet exist for a player who has not logged out or been saved.
- `trail check` currently flags broad lexical matches in existing dev changelog and command registrations. Inspect actual changed lines; this change keeps the focused roster and does not reintroduce latest.log parsing or generic file listing.
