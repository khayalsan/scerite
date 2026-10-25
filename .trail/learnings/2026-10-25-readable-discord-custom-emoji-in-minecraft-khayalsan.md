---
title: Readable Discord custom emoji in Minecraft chat
author: Khayalsan
date: 2026-10-25
branch: feat/discord-bridge
touches:
  - src-tauri/src/discord_bridge.ts
---

## Decided
Convert Discord custom emoji references to their readable `:name:` form before relaying a Discord chat message into Minecraft. Handle `<:name:id>`, `<a:name:id>`, and `<name:id>` while leaving Unicode emoji and ordinary text unchanged.

## Tried and ruled out
Forwarding message.content directly exposes Discord's raw emoji IDs in Minecraft chat; the Discord client renders those references but Minecraft does not.

## Watch out
Normalize only the message content before applying the configured Minecraft template. Do not parse or rewrite usernames, custom template syntax, or other Discord mentions.
