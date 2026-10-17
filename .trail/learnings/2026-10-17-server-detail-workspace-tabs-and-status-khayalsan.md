---
title: Server detail workspace tabs and status
author: Khayalsan
date: 2026-10-17
branch: feat/server-detail-pages
touches:
  - src/App.css
  - src/App.tsx
---

## Decided
Keep server status and Start/Stop controls at the top of the detail sidebar. Use Console, Files, and Settings panels for the server's existing operations, and hide inactive panels so console state and file editor drafts stay mounted while switching tabs.

## Tried and ruled out
Unpublishing or filtering inactive detail content would unmount LocalFilesPanel and can lose unsaved text. Console should connect only when selected explicitly, using the existing server session path.

## Watch out
Status comes from local session state or remote SSH status; an unchecked remote server is not yet confirmed reachable. Trail phrase matches for profile/local process, remote SSH, file editing, and console output here are UI references to existing tmux-backed session controls, contained file commands, and in-memory output. They do not add a profile database, foreground remote launch, unsafe path access, or persisted console history. The remote address in the heading is display-only.
