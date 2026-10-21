---
title: World backup visibility and gamerule storage formats
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
Worlds tab lists both app-data ZIP backups and server-side preserved folders so a backup has a visible location. Read gamerules from Data.GameRules, Data.game_rules, or data/minecraft/game_rules.dat according to the saved world's format; allow namespaced rule IDs. Keep the Worlds tab as a bounded scroll region.

## Tried and ruled out
A static gamerule catalog was not used because Minecraft versions and loaders can change the available rules and their values. Showing only Data.GameRules misses newer worlds.

## Watch out
Minecraft 26.1 moved game rules to data/minecraft/game_rules.dat. Remote backup listing needs SFTP to show preserved server folders; ZIP backups live in desktop app data.
