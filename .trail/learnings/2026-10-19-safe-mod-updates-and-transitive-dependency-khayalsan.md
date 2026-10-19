---
title: Safe mod updates and transitive dependency removal
author: Khayalsan
date: 2026-10-19
branch: feat/server-players-logs
touches:
  - CHANGELOG.md
  - src-tauri/src/lib.rs
  - src-tauri/src/modrinth.rs
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/server_profiles/model.rs
  - src-tauri/src/server_profiles/store.rs
  - src/components/PackagesPanel.tsx
  - src/lib/serverProfiles.ts
---

## Decided
Modrinth-managed mods use the existing verified download-before-old-JAR-removal update path and the guarded local/remote package removal path. New installed records save direct dependency project IDs; older records fall back to their saved requiredBy names. Removal preview walks reverse dependencies transitively, blocks removal while any dependent remains, and offers only non-direct dependencies whose remaining parents are all in the removal set.

## Tried and ruled out
A direct requiredBy label alone cannot show A and B when A requires B and B requires C. Automatically removing every dependency named by the selected mod can delete shared dependencies. Deleting a file before verifying its published checksum or while the server runs is unsafe.

## Watch out
Keep cleanup optional and recheck the approved dependency list before deletion. An interrupted cleanup can leave unused JARs; refresh the installed list and report the partial result. Legacy title links can be ambiguous, so unresolved saved dependents remain blockers instead of being assumed unused. Do not enable updating a dependency while other installed mods require it.
