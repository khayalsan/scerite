---
title: Russh migration supersedes the WinCNG limitation
author: Khayalsan
date: 2026-10-17
branch: feat/local-server-operations
touches:
  - src-tauri/Cargo.lock
  - src-tauri/Cargo.toml
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/server_sessions.rs
---

## Decided
- The scerite SSH implementation now uses russh with the ring backend, replacing ssh2/libssh2 and its Windows OpenSSL build path. This supersedes the earlier finding that the previous backend lacked the needed modern Windows SSH algorithms.
- Keep the older security and lifecycle constraints: verify host keys against known_hosts, reject untrusted host certificates, avoid legacy algorithm fallbacks, and use tmux for persistent Minecraft processes.

## Tried and ruled out
- Changing only libssh2 host-key preferences cannot add missing Curve25519 or Ed25519 support; the backend was replaced instead.

## Watch out
- A compile check does not prove real-server interoperability. Exercise the user's Linux host and confirm both SSH-agent/key-file authentication and SFTP POSIX rename behavior before release.
