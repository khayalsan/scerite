---
title: Bound local server console history
author: Khayalsan
date: 2026-10-17
branch: feat/local-server-operations
touches:
  - src-tauri/src/server_sessions.rs
  - src/lib/serverSessions.ts
  - src/App.tsx
---

## Decided
Keep local server output in a bounded in-memory ring buffer owned by the detached session manager. Fetch new lines by sequence number and write one validated command line to the server process stdin.

## Tried and ruled out
Do not persist console output to profile storage. Do not turn console commands into shell commands; they go directly to the child process stdin.

## Watch out
The ring buffer is capped at 2,000 lines and 512 KB; old lines are discarded. Keep command input newline-free and bounded. Remote console transport remains a separate implementation.
