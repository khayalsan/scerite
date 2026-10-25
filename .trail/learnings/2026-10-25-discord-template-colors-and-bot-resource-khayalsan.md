---
title: Discord template colors and bot resource usage
author: Khayalsan
date: 2026-10-25
branch: feat/discord-bridge
touches:
  - src-tauri/src/discord_bridge.ts
  - src-tauri/src/remote_ssh.rs
  - src/components/DiscordBridgeSettings.tsx
---

## Decided
Discord-to-Minecraft templates use `{username}` for the Discord account username, `{name}` for the display name, and `{message}` for content. The default wraps `{name}` in `{color:blue}` and `{color:reset}`. The bridge emits separate tellraw text components so template color tags style only their spans; names and message contents are inserted as plain text. The single Node bridge per environment reports CPU and RSS from its live tmux pane PID through the status command.

## Tried and ruled out
Embedding legacy formatting characters in a plain tellraw text string would not give reliable structured colors. Polling bridge status continuously is unnecessary; usage is fetched when settings opens and by an explicit refresh button.

## Watch out
Only template text is parsed for placeholders and colors; never parse inserted Discord content as markup. Older saved default templates are upgraded, while custom templates remain. The Node launch uses exec, so tmux pane_pid identifies the Node process; a missing/dead pane leaves usage unavailable.
