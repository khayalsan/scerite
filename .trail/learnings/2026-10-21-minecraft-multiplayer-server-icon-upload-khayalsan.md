---
title: Minecraft multiplayer server icon upload
author: Khayalsan
date: 2026-10-21
branch: feat/server-list-icon
touches:
  - CHANGELOG.md
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/server_files.rs
  - src/App.css
  - src/components/ServerOverviewPanel.tsx
  - src/components/ServerIconControl.tsx
---

## Decided
The Server tab accepts a local image, center crops it to 64x64 PNG in the webview, and sends PNG bytes to dedicated local or remote Tauri commands. The backend writes only server-icon.png in the managed server root after confirming the server is stopped.
## Tried and ruled out
The generic text file editor cannot handle PNG bytes. A custom remote agent or extra image library is unnecessary for this single icon upload.
## Watch out
Keep symlink and server-running checks in the backend. Minecraft applies a replaced icon after server start. Trail check may match existing world and roster entries on dev; review the actual feature diff.
