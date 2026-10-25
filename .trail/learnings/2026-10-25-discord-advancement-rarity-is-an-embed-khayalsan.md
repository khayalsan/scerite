---
title: Discord advancement rarity is an embed accent
author: Khayalsan
date: 2026-10-25
branch: feat/discord-bridge
touches:
  - src-tauri/src/discord_bridge.ts
---

## Decided
Discord embed title text cannot be colored per word. Keep the advancement name as plain title text and show task, goal, or challenge color through the embed accent border. Do not add colored-circle emoji as a substitute; it renders as a separate clickable image-like glyph and is not what the user requested.
## Tried and ruled out
The prior colored emoji marker made Postmortal appear as a linked yellow circle before the title. ANSI code blocks would move the name out of the title and look unlike a normal embed.
## Watch out
Keep the existing title and description layout and frame-specific embed color when adjusting advancement notifications.
