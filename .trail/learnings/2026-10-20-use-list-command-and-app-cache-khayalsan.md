---
title: Use list command and app cache for online players
author: Khayalsan
date: 2026-10-20
branch: fix/server-port-conflicts
touches:
  - CHANGELOG.md
  - src-tauri/src/lib.rs
  - src/components/PlayersPanel.tsx
  - src/lib/serverPlayers.ts
  - src-tauri/src/online_player_cache.rs
  - src/lib/onlinePlayers.ts
---

## Decided
- The Players tab displays the saved roster as soon as the roster read finishes. A separate `list` console command determines current online names for local and SSH-managed servers.
- Save successful `list` responses and names in per-profile JSON files under Scerite app data. Cached names supplement the offline roster, but cached online state is not treated as live when reopening the tab.
## Tried and ruled out
- Inferring online names by scanning join and leave messages in latest.log is superseded. It could miss joins when the log is truncated, and it held the roster display until an extra read completed.
- Do not expose RCON or a new network management port for this feature.
## Watch out
- Console output formats may vary with Minecraft versions and plugins. Only cache a response when the parsed name count matches the reported count. Keep a bounded wait and report unavailable status if the command response cannot be read.
- The remote console is a tmux pane. A pane outside Scerite's managed session cannot answer this command through the current path.
