---
title: Replace OpenSSL SSH transport with russh
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
- Remote SSH and SFTP use russh and russh-sftp with russh's ring backend; ssh2 and OpenSSL are removed from the dependency graph.
- Keep known_hosts verification strict. The client rejects host certificates unless a trusted CA flow is implemented, and supports key files plus the platform SSH agent.
- Keep remote controls on persistent tmux sessions and preserve rooted paths, symlink rejection, bounded file reads, and temporary-file saves.
- Use the SFTP POSIX rename extension for atomic overwrite. If the server does not support it, leave the destination unchanged and report an error.

## Tried and ruled out
- The high-level russh-sftp rename API has no explicit atomic-overwrite flags, so overwrite uses its raw SFTP extension request instead of removing the destination first.
- Vendored OpenSSL was the source of Windows Perl/build-tool friction and is no longer needed for SSH.

## Watch out
- Verify interoperability against a real Linux SSH/SFTP host, including known_hosts matching, agent and key-file auth, tmux commands, file containment, and POSIX rename support.
- The Windows cargo check verifies compilation only; it does not exercise SSH negotiation or agent connections at runtime.
