---
title: Global EULA option applies at server start
author: Khayalsan
date: 2026-10-18
branch: feat/vanilla-server-installation
touches:
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/server_profiles/commands.rs
  - src-tauri/src/server_profiles/store.rs
  - src-tauri/src/server_sessions.rs
  - src/App.css
  - src/App.tsx
  - src/lib/serverProfiles.ts
  - src-tauri/src/eula.rs
---

## Decided
Auto enable EULA is one application setting shared by local and remote servers, enabled by default for new and existing saved settings. When enabled, ServerCrafter writes eula=true in the selected server's eula.txt immediately before starting it. The setting description is the user's exact wording.

## Tried and ruled out
Setting EULA only during installation misses servers added from existing folders, and a frontend-only preference would leave the backend launch paths unaware of it.

## Watch out
Only touch eula.txt while the selected server is stopped. Reject symbolic links and non-file targets, preserve unrelated lines, and do not rewrite a file already containing only an enabled EULA value. Remote starts remain in tmux and use the configured server directory.
