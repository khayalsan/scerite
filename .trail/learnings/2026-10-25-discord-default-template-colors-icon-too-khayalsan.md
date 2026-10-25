---
title: Discord default template colors icon too
author: Khayalsan
date: 2026-10-25
branch: feat/discord-bridge
touches:
  - src-tauri/src/discord_bridge.ts
  - src-tauri/src/remote_ssh.rs
  - src/components/DiscordBridgeSettings.tsx
---

## Decided
The Discord player-death action label is simply `Player deaths`; PvP is already included. The default Discord-to-Minecraft template is exactly `<{color:blue}🎮{name}{color:reset}> {message}`, so the icon and display name share the blue color.

## Tried and ruled out
Placing `{color:blue}` after the game icon left the icon uncolored. Calling out PvP in the Player deaths label repeated what the action already means.

## Watch out
Migrate only the two exact older default template strings in both status normalization and bridge runtime. Keep any user-customized template unchanged.
