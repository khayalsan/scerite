---
title: Stage Minecraft player identities before full roster
author: Khayalsan
date: 2026-10-21
branch: fix/fast-player-roster
touches:
  - CHANGELOG.md
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/server_players.rs
  - src/components/PlayersPanel.tsx
  - src/lib/serverPlayers.ts
  - src-tauri/src/player_identity_cache.rs
---

## Decided
Show per-server cached player identities from Scerite app data before remote roster work. On a cold load, read usercache.json in a focused command before enumerating player files. Treat whitelist, operator, ban, and online status as pending until their authoritative reads finish.

## Tried and ruled out
Do not cache access flags as current facts. Do not use latest.log for online names. The focused usercache read adds one remote SFTP operation on a cold load, but avoids waiting for four directory listings before showing names.

## Watch out
The cache accelerates later visits; the first visit with no cache or usercache may still wait for the full roster. Keep remote path and symlink checks through optional_remote_player_file. Trail check flags lexical matches against older server file and SSH learnings; this change does not use generic file listing, launch a server over SSH, or change profile storage.
