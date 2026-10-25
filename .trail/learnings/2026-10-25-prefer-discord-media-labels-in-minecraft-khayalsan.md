---
title: Prefer Discord media labels in Minecraft chat
author: Khayalsan
date: 2026-10-25
branch: feat/discord-bridge
touches:
  - src-tauri/src/discord_bridge.ts
---

## Decided
When relaying Discord URLs, use the matching embed title as Minecraft clickable text. For GIF embeds, use a short description when no title is present. Uploaded media uses attachment.description. Fall back to the site or generic media type if Discord provides no label. Cap and sanitize labels before tellraw.
## Tried and ruled out
Displaying only the provider name loses the user-visible label chosen for GIFs such as Klipy. Fetching remote page metadata would add network requests and is unnecessary when Discord already supplies preview metadata.
## Watch out
Discord.js Embed exposes the embed type through embed.data.type, not embed.type. Preview metadata may be absent; retain a fallback.
