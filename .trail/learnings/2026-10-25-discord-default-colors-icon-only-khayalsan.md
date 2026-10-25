---
title: Discord default colors icon only
author: Khayalsan
date: 2026-10-25
branch: feat/discord-bridge
touches:
  - src-tauri/src/discord_bridge.ts
  - src-tauri/src/remote_ssh.rs
  - src/components/DiscordBridgeSettings.tsx
---

## Decided
The Discord-to-Minecraft default is exactly `<{color:blue}🎮{color:reset}{name}> {message}`: the game icon is blue and the display name uses the normal chat color. The `Player deaths` action label stays concise.

## Tried and ruled out
The prior default `<{color:blue}🎮{name}{color:reset}> {message}` colored both icon and name; the user changed that preference.

## Watch out
Upgrade only exact previous default strings, including the icon-and-name-blue variant, in both status normalization and bridge runtime. Preserve custom templates.
