---
title: Discord to Minecraft message template
author: Khayalsan
date: 2026-10-25
branch: feat/discord-bridge
touches:
  - src-tauri/src/discord_bridge.ts
  - src-tauri/src/remote_ssh.rs
  - src/components/DiscordBridgeSettings.tsx
---

## Decided
Discord-to-Minecraft chat uses one per-server template, defaulting to `<🎮{username}> {message}`. Replace only `{username}` and `{message}` in the template, then send the resulting plain text with `tellraw` to avoid vanilla's `[Server]` prefix. Existing remote bridge configs without the field receive the same default in status and runtime paths.

## Tried and ruled out
Hardcoding a `[Discord] name: message` prefix cannot meet the requested custom format.

## Watch out
The remote bridge script and settings JSON are deployed separately; an updated app must redeploy the bot for new formatting code. Keep the frontend default, Rust status fallback, generated JSON, and TypeScript runtime fallback aligned. Require `{message}` in custom templates.
