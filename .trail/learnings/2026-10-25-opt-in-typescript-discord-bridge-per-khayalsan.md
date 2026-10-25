---
title: Opt-in TypeScript Discord bridge per remote environment
author: Khayalsan
date: 2026-10-25
branch: feat/discord-bridge
touches:
  - CHANGELOG.md
  - package-lock.json
  - package.json
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src/App.css
  - src/App.tsx
  - src-tauri/src/discord_bridge.ts
  - src/components/DiscordBridgeSettings.tsx
  - tsconfig.bridge.json
---

## Decided
The Discord bridge is opt-in per remote environment. Author it in TypeScript, type-check it in the app build, and run it with a managed Node 24 runtime in a persistent tmux session. Each server maps to its own Discord channel. Keep the user-provided bot token in a mode-600 remote file, separate from local profiles and command lines.
## Tried and ruled out
A Python bridge adds another runtime and language to Scerite. Running the bridge inside the desktop app would stop chat sync when the app closes. A per-server bot would duplicate the Discord connection and runtime.
## Watch out
A Minecraft tmux session can remain after its pane dies, so check pane_dead rather than has-session for server status. Only download Node and npm packages after the user enables the bridge. Log polling is for chat notifications, not the Players roster; bound each read and avoid replaying old logs.
