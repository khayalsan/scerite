---
title: Explain unsaved player data and resolve heads by name
author: Khayalsan
date: 2026-10-20
branch: fix/server-port-conflicts
touches:
  - src-tauri/src/server_players.rs src/lib/serverPlayers.ts src/components/PlayersPanel.tsx
---

## Decided
- The Players tab reads saved NBT and statistics files, so a first join can be known from the roster before saved details exist. Return explicit saved-file presence flags and explain that state rather than showing empty fact cards.
- For the visible head, prefer the validated player name with MCHeads, which resolves usernames and UUIDs; the previous UUID-only Crafatar request could show a default skin when the server UUID was not resolvable there.

## Tried and ruled out
- Treating missing saved files as empty statistics made first-join behavior look like a parser failure.
- Polling server files repeatedly cannot create data the Minecraft server has not saved yet and would add remote SSH traffic.

## Watch out
- The head remains a best-effort external lookup and can lag behind recent skin changes or differ for custom skin plugins.
- Refresh the player after the server saves their data to load newly written files.
