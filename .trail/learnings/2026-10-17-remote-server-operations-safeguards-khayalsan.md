---
title: Remote server operations safeguards
author: Khayalsan
date: 2026-10-17
branch: feat/local-server-operations
touches:
  - README.md
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/server_files.rs
  - src-tauri/src/server_profiles/mod.rs
  - src-tauri/src/server_profiles/model.rs
  - src-tauri/src/server_sessions.rs
  - src/App.tsx
  - src/components/LocalFilesPanel.tsx
  - src/lib/serverProfiles.ts
---

## Decided
Remote server operations extend the existing local controls using SSH, tmux, and SFTP. Keep SSH host verification tied to known_hosts, allow agent or selected key-file authentication, and keep remote paths rooted in the configured server directory. Use tmux sessions so server processes survive app exit; detect unmanaged Java processes before launch or file writes.

## Tried and ruled out
Do not launch remote servers as transient SSH foreground commands: disconnecting SSH must not stop Minecraft. Do not accept unknown or changed host keys automatically. Do not route console input through a shell command; send it to the tmux session. Do not provide arbitrary remote filesystem access or file writes while a server is running.

## Watch out
Older local-operations learnings may flag shared console, profile, and file-operation identifiers. They describe the local implementation scope; this remote extension preserves the same boundaries and adds remote transport. Live remote-server integration was not available during implementation, so validate against a real Linux host with tmux and OpenSSH before release.
