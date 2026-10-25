---
title: Resolve vanilla advancement descriptions for Discord embeds
author: Khayalsan
date: 2026-10-25
branch: feat/discord-bridge
touches:
  - src-tauri/src/discord_bridge.ts
---

## Decided
The Minecraft log announces an advancement title and whether it is a task, goal, or challenge, but not its hover description. Build Discord embed titles from the log phrase, use green, gold, or purple markers and accent colors by frame, and load English vanilla advancement descriptions from the matching official Mojang client language file. Cache only the small title-to-description lookup beside the opted-in bridge. If version metadata or download is unavailable, send the title without a description.
## Tried and ruled out
A hardcoded Monster Hunter description or a fixed list would fail on other advancements and versions. The server jar does not provide the English client language file. Adding a Discord-specific plugin or port is unnecessary.
## Watch out
The first lookup downloads the matching Minecraft client jar once on the remote host and may delay that announcement; subsequent lookups use the cached JSON. Modded or custom advancement descriptions are not covered by vanilla translations. Discord embeds color the border, not individual title words, so the title also uses a colored circle.
