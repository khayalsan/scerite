---
title: Playit turn off versus delete
author: Khayalsan
date: 2026-10-26
branch: feat/connection-method-navigation
touches:
  - CHANGELOG.md
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src/components/PlayitSetupPanel.tsx
---

## Decided
Playit Turn off stops the shared remote tmux agent and removes only its enabled marker. A separate confirmed Delete action stops it, deletes the managed binaries and saved account link, removes the socket, and removes the runtime directory if it is empty. The Playit status exposes installation separately from enabled state so Delete remains available after Turn off.
## Tried and ruled out
Making Turn off delete the account link would force users to claim the agent again on restart. Recursively deleting the runtime directory could remove unknown files, so Delete removes only known managed files and leaves any other contents untouched. Scerite does not delete tunnels from the user's Playit account.
## Watch out
One agent is shared by all servers in an environment. Both Turn off and Delete disconnect their Playit tunnels. Check the remote runtime path and file types before deletion; never follow symlinks or assume the directory contains only managed files.
