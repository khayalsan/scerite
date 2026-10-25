---
title: Discord to Minecraft delivery uses one tmux command
author: Khayalsan
date: 2026-10-25
branch: feat/discord-bridge
touches:
  - CHANGELOG.md
  - src-tauri/src/discord_bridge.ts
---

## Decided
Send Discord chat to a Minecraft tmux pane as one `tmux send-keys` invocation with the full `say` argument followed by the Enter key. Retry only the pre-send pane availability check briefly and log a failed Discord message ID; do not retry after a send attempt because tmux may have accepted it.
## Tried and ruled out
The earlier path silently returned on one failed `hasSession` check and sent text and Enter in separate tmux invocations. A euro sign alone was not supported as the cause because resending the same content worked.
## Watch out
Without the remote bridge log from the failed moment, a particular lost message cannot be diagnosed conclusively. tmux accepting keys does not prove Minecraft processed the command. The `say ` prefix makes the whole text argument an unrecognized key name, so tmux sends its UTF-8 characters before the separate Enter key in the same invocation.
