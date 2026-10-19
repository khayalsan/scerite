---
title: Rename folders with server names and show server overview
author: Khayalsan
date: 2026-10-19
branch: feat/release-changelog
touches:
  - CHANGELOG.md
  - README.md
  - src-tauri/Cargo.lock
  - src-tauri/Cargo.toml
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/server_profiles/commands.rs
  - src-tauri/src/server_profiles/store.rs
  - src/App.css
  - src/App.tsx
---

## Decided
The user changed the earlier rule: changing a server profile name now renames its local or remote folder to the new name's slug. Require the server to be stopped, reject an existing destination or shared source folder, preserve installed software metadata, and move the folder back if profile persistence fails. Keep the Server overview as the first tab with status, address, software/version, players from the Minecraft status protocol, and process CPU/RAM.

## Tried and ruled out
The older managed-folder learning said not to rename folders on a profile name change. That was the previous behavior and is superseded by this user request. A status ping failure is ordinary unavailability and must not invalidate an otherwise usable SSH connection.

## Watch out
Remote player counts use SSH direct TCP forwarding to 127.0.0.1; SSH servers that disallow forwarding will show the count as unavailable. CPU/RAM refer to the Java process, not the whole host. Imported profiles may have unknown software/version metadata.
