---
title: Use one SFTP scan for world discovery
author: Khayalsan
date: 2026-10-18
branch: feat/vanilla-server-installation
touches:
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/server_files.rs
  - src/components/WorldsPanel.tsx
  - src/lib/serverFiles.ts
---

## Decided
World discovery now uses dedicated local and remote commands that inspect only the server root and each direct child level.dat metadata. The remote path reuses one SSH connection and one SFTP subsystem for the scan. The frontend shows the world list before loading server.properties for its active-world label.

## Tried and ruled out
Reusing list_server_files for every candidate folder created one remote command and SFTP subsystem per folder, listed unnecessary contents, and made loading slow. Sequential calls avoided concurrent invalidation but increased latency further. A direct metadata check is enough to identify world folders.

## Watch out
The scan still checks each root directory for level.dat, so it is bounded by top-level folder count and network latency. Skip symlinks and keep all paths under the configured server root. World contents should load only after selection.
