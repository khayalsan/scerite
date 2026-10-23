---
title: Show saved multiplayer icons in server list
author: Khayalsan
date: 2026-10-23
branch: feat/server-list-icons
touches:
  - CHANGELOG.md
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/server_files.rs
  - src/App.css
  - src/App.tsx
  - src/components/ServerListIcon.tsx
---

## Decided
- The Servers list reads each profile's server-icon.png through a small dedicated Tauri command. Keep file access in Rust for local and SSH/SFTP remote profiles.
- Show the saved multiplayer icon in the list row; use the existing local/remote glyph when the file is missing, invalid, or unreachable.
- Read on row mount without polling. Returning from a server detail page remounts the row and picks up an icon saved there.
## Tried and ruled out
- Reading the PNG through the generic server-file command: that endpoint is UTF-8 text and cannot carry binary images.
- Building remote paths in React: remote profiles already have a managed root resolved by the SSH backend.
## Watch out
- Accept only regular 64x64 PNG files up to 1 MB. Remote symlinks and oversized files must not be read for a decorative list image.
- Revoke browser object URLs when rows unmount.
