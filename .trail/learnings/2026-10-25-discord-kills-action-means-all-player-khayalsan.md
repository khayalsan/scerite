---
title: Discord kills action means all player deaths
author: Khayalsan
date: 2026-10-25
branch: feat/discord-bridge
touches:
  - CHANGELOG.md
  - src-tauri/src/discord_bridge.ts
  - src/components/DiscordBridgeSettings.tsx
---

## Decided
The Discord bridge's kills action means player death announcements from the normal Minecraft log, including PvP, mobs, and environmental causes. Preserve the reported cause in the Discord post. The configuration key remains kills for compatibility, but the UI calls it Player deaths.
## Tried and ruled out
PvP-only filtering missed ordinary death messages and was narrower than the user's intended action.
## Watch out
Match only known player victims from usercache or join events, so unrelated log lines do not become death notifications. This supersedes the PvP-only part of the earlier Discord routing learning.
