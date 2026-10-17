---
title: Constrain local server file access
author: Khayalsan
date: 2026-10-17
branch: feat/local-server-operations
touches:
  - src-tauri/src/server_files.rs
  - src/components/LocalFilesPanel.tsx
  - src/App.tsx
---

## Decided
Perform server file reads and writes in Rust, rooted at the canonical local profile folder. Expose directory listing plus bounded UTF-8 text editing. Require the server to be stopped before any save, and ask for confirmation before replacing an existing file.

## Tried and ruled out
Do not expose arbitrary absolute paths, parent traversal, or symbolic-link writes. Do not edit files over 1 MB or persist them outside the server directory.

## Watch out
The UI confirmation is paired with backend containment checks. New files can only use an existing parent folder. Add directory creation or deletion as a separately reviewed operation.
