---
title: Use vanilla tellraw for Discord chat without Server prefix
author: Khayalsan
date: 2026-10-25
branch: feat/discord-bridge
touches:
  - CHANGELOG.md
  - src-tauri/src/discord_bridge.ts
---

## Decided
Vanilla Minecraft adds `[Server]` to console `say` output. The Discord bridge uses `tellraw @a` with a single escaped plain-text component instead, retaining `[Discord] name: message` without the automatic sender label. JSON-style quoted object and string syntax is also valid for modern SNBT text components; no plugin or extra port is needed.
## Tried and ruled out
A server setting cannot remove `[Server]` from `say`. Using a plugin for chat formatting is unnecessary for this display change. Never concatenate raw Discord content into command structure; build the component with JSON.stringify after removing control characters.
## Watch out
Minecraft 1.21.5 changed text component command input from JSON to SNBT; avoid text-component features whose formats diverge across versions. The bridge still sends the whole command and Enter in one tmux invocation.
