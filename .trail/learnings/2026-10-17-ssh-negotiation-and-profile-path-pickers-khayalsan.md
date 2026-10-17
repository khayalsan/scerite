---
title: SSH negotiation and profile path pickers
author: Khayalsan
date: 2026-10-17
branch: feat/local-server-operations
touches:
  - package-lock.json
  - package.json
  - src-tauri/Cargo.lock
  - src-tauri/Cargo.toml
  - src-tauri/capabilities/default.json
  - src-tauri/src/lib.rs
  - src-tauri/src/main.rs
  - src-tauri/src/remote_ssh.rs
  - src/App.css
  - src/App.tsx
---

## Decided
For the libssh2 handshake, configure host-key preferences from the library's supported algorithms but remove OpenSSH host-certificate algorithm names that libssh2 advertises without verification support. Keep standard host keys and existing known_hosts verification. In profile editing, test the unsaved SSH connection values; native path pickers apply to local server folders and local identity key files.

## Tried and ruled out
Do not bypass host-key verification or enable legacy SSH algorithms to work around negotiation errors. A native desktop folder picker cannot select a path on a remote Linux host; keep that field as an absolute remote path.

## Watch out
`Unable to exchange encryption keys` occurs before host-key verification and authentication, and libssh2 can report it for more than one algorithm incompatibility. If filtering unsupported certificate host-key advertisements does not fix a specific server, compare its SSH offer using OpenSSH verbose output and add support without weakening trust checks. The user must retry against their instance to confirm compatibility.
