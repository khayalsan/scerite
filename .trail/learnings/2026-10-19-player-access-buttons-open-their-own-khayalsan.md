---
title: Player access buttons open their own lists
author: Khayalsan
date: 2026-10-19
branch: feat/server-players-logs
touches:
  - CHANGELOG.md
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/server_players.rs
  - src/App.css
  - src/components/PlayersPanel.tsx
  - src/lib/serverPlayers.ts
---

## Decided
- In the Players tab, Whitelist, OPs, Ban by name, and Ban by IP are navigation buttons. Each opens its own current-entry list with an add form and a remove action for each entry.
- Keep the known-player roster and player detail view separate from access management. Read banned-ips.json alongside the other server access lists; send changes through the running console.
## Tried and ruled out
- A single inline player-name and IP form with all commands exposed at once did not match the requested flow.
## Watch out
- List writes are made by the Minecraft server after a command is sent. Refresh the list after the server saves it; do not edit the JSON files directly while the server runs.
