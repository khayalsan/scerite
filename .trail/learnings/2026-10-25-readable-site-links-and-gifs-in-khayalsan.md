---
title: Readable site links and GIFs in Discord relay
author: Khayalsan
date: 2026-10-25
branch: feat/discord-bridge
touches:
  - src-tauri/src/discord_bridge.ts
---

## Decided
Relayed Discord chat renders HTTP(S) URLs inside `{message}` as short clickable site labels while preserving surrounding text. Recognize common hosts such as YouTube, Tenor, and Giphy; use the hostname otherwise. If a user GIF/video arrives as an embed without a URL in message content, relay its embed URL once. Uploaded files retain media links, with uploaded GIFs labeled `[GIF]`.

## Tried and ruled out
Sending a long raw GIF or video URL into Minecraft chat is noisy. Relaying both a pasted URL and its generated preview duplicates the link. Arbitrary non-HTTP schemes cannot be used for vanilla click events.

## Watch out
Keep URL parsing confined to the `{message}` value, not template tokens or Discord names. Use JSON.stringify for tellraw components. Bound visible text and clickable link count, and do not let an auto-generated embed duplicate uploaded attachments or a URL already in content.
