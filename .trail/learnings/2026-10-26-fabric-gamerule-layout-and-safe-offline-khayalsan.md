---
title: Fabric gamerule layout and safe offline editing
author: Khayalsan
date: 2026-10-26
branch: fix/world-gamerules-layout
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
Worlds now reads both direct and nested data in game_rules.dat. When a server is stopped, a gamerule can be changed in saved NBT; running servers still receive a console command. The user explicitly requested offline edits, updating the earlier live-only decision.

## Tried and ruled out
The previous live-only approach left gamerules uneditable on stopped Fabric and Paper servers. Do not write raw text to NBT or use the generic UTF-8 file editor; decode and re-encode the existing compound, validate the existing rule type, and replace via a temporary file. For legacy worlds, level.dat can be edited only while stopped, with a recovery copy during replacement.

## Watch out
Modern game_rules.dat may store rules directly under data rather than data.gamerules. Preserve all other NBT tags, bound reads, reject symlinks and running processes, and keep live rule changes on the console path. Remote updates use SFTP POSIX rename. The world type in server.properties may escape its colon as minecraft\:normal; unescape it for the selector.
