---
title: Discover and inspect world folders
author: Khayalsan
date: 2026-10-18
branch: feat/vanilla-server-installation
touches:
  - src/App.css
  - src/App.tsx
  - src/components/WorldsPanel.tsx
---

## Decided
The Worlds tab detects server-root folders containing level.dat through the existing local/remote file listing commands, then offers read-only browsing and text preview. server.properties level-name labels the active world.

## Tried and ruled out
A new Rust command is unnecessary for this read-only first slice; existing file APIs already enforce profile-root containment and work over SFTP.

## Watch out
World data files such as level.dat are binary and cannot be text-previewed. Worlds nested outside the server root are not detected. Keep the Worlds tab read-only unless a later request explicitly adds mutation and safeguards.
