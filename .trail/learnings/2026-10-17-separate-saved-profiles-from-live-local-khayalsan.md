---
title: Separate saved profiles from live local sessions
author: Khayalsan
date: 2026-10-17
branch: feat/local-server-operations
touches:
  - src-tauri/src/server_sessions.rs
  - src-tauri/src/server_profiles/model.rs
  - src-tauri/src/lib.rs
---

## Decided
Keep saved profile configuration in the existing serde_json store. Run local Minecraft processes under a detached Rust supervisor, with a per-install local IPC token so the desktop app can reconnect after restart. The token and live Child handles are runtime session state, not a replacement profile store.

## Tried and ruled out
A separate profile database or storage plugin is unnecessary for local process control. Reusing the GUI process for Minecraft would not survive app exit.

## Watch out
Keep IPC local-only, validate profile paths and launch inputs, avoid logging credentials, and stop servers through Minecraft's graceful stop command.
