---
title: Safe updates and removal for managed plugins
author: Khayalsan
date: 2026-10-19
branch: feat/plugin-update-removal
touches:
  - README.md
  - src-tauri/src/lib.rs
  - src-tauri/src/modrinth.rs
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/server_profiles/store.rs
  - src/App.css
  - src/components/PackagesPanel.tsx
---

## Decided
Modrinth-managed plugins can be updated or removed while the server is stopped. For an update, resolve compatible versions and dependencies, install and checksum-verify a distinct new JAR, then verify and remove the old JAR. Keep the old JAR if installation or cleanup fails. Removal accepts only a tracked plugin whose file matches the saved Modrinth version checksum; refuse removal when another installed package requires it. Local and remote Linux paths use the same profile and server-status checks.

## Tried and ruled out
Deleting the old JAR before downloading its replacement risks leaving a server without the plugin. Automatically deleting untracked JARs or modified tracked JARs lacks reliable provenance. Automatically updating a dependency used by another package risks breaking that package; require a compatible plan or stop with an actionable error.

## Watch out
Both JARs can remain when the new version installs but old-file cleanup fails. Retry while the server is stopped after reviewing the installed list. The package store records project and version IDs, while dependency references use project titles; reconcile stale references after an update. Trail check currently reports keyword-based conflicts here against unrelated older learnings; the update uses the existing Modrinth compatibility resolver and does not add a database, loader inference, or arbitrary filesystem writes.
