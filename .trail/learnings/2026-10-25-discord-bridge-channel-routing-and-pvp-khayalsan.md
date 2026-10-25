---
title: Discord bridge channel routing and PvP events
author: Khayalsan
date: 2026-10-25
branch: feat/discord-bridge
touches:
  - CHANGELOG.md
  - src-tauri/src/discord_bridge.ts
  - src-tauri/src/remote_ssh.rs
  - src/App.css
  - src/components/DiscordBridgeSettings.tsx
  - package.json
---

## Decided
Keep one TypeScript Discord bot per remote environment. Channel setup is optional; each action can inherit the server default channel or override it. Chat has separate send and receive controls. PvP kills are inferred from death lines only when the killer is a known player from usercache or a join event; advancements come from normal log announcements.
## Tried and ruled out
Requiring a channel before enabling blocked token-only setup. Treating every death-message killer string as a player could mislabel mob kills.
## Watch out
The remote managed package.json must declare type module before npm installation, including upgrades of existing bridge installations. Old servers.json uses chat/status/joins and needs status normalization when editing.
