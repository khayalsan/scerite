---
title: Separate remote environments from server profiles
author: Khayalsan
date: 2026-10-17
branch: feat/server-environments
touches:
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/server_profiles/commands.rs
  - src-tauri/src/server_profiles/mod.rs
  - src-tauri/src/server_profiles/model.rs
  - src-tauri/src/server_profiles/store.rs
  - src/App.css
  - src/App.tsx
  - src/lib/serverProfiles.ts
---

## Decided
- Save reusable SSH connection settings in a remote environment. Link each remote server profile to one environment while keeping its name, directory, and launch settings per server. Keep local profiles standalone.
- On startup, migrate legacy remote profiles into environments and group only profiles with identical host, port, user, and identity-file settings.

## Tried and ruled out
- Repeating SSH connection details on every remote server keeps the existing duplication and makes shared connection changes error-prone.

## Watch out
- Require the referenced environment to exist and prevent deleting environments that still contain servers. Keep each server's SFTP root scoped to its own configured directory.
