---
title: Discord bot update and player webhook chat
author: Khayalsan
date: 2026-10-25
branch: feat/discord-bridge
touches:
  - CHANGELOG.md
  - src-tauri/src/discord_bridge.ts
  - src-tauri/src/remote_ssh.rs
  - src/components/DiscordBridgeSettings.tsx
---

## Decided
An enabled remote Discord bridge reports an update when its installed TypeScript script differs from the app-bundled script; the Update bot button redeploys with the saved token and channel mapping. For Minecraft-to-Discord chat, reuse a bot-owned webhook per text channel so each message can use the player's name and MCHeads skin avatar. Joins, leaves, deaths, and advancements use embeds, while online/offline notices keep their existing format.
## Tried and ruled out
Creating a new webhook for every message or restart would accumulate Discord integrations. Requiring webhook permission without a fallback would silently lose existing chat routes. Fetch owned webhooks first; use normal bot text if Manage Webhooks is unavailable.
## Watch out
The bot needs Manage Webhooks in each text channel for player-style messages. Do not log webhook tokens; filter fetched hooks by owner and name. Webhook messages are ignored by the Discord-to-Minecraft relay to prevent loops. This branch currently extends open PR 37 rather than dev directly.
