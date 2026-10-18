---
title: Use both OpenSSH authentication choices
author: Khayalsan
date: 2026-10-17
branch: feat/local-server-operations
touches:
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/server_profiles/model.rs
  - src/App.tsx
---

## Decided
The user chose both remote authentication options. Leave the SSH identity path blank to use the OS SSH agent/default identities; an optional per-profile identity file selects a key path. Use the system OpenSSH client with BatchMode and StrictHostKeyChecking so secrets are not persisted and host keys must be trusted in known_hosts.

## Tried and ruled out
Do not save private-key passphrases in profile JSON. Do not auto-accept unknown or changed host keys. The initial remote slice tests connectivity only; it does not yet manage tmux sessions or remote files.

## Watch out
OpenSSH must be installed and on PATH. Encrypted identity files need to be loaded into the SSH agent before scerite can use them. The user should verify a server fingerprint before adding it to known_hosts.
