---
title: Do not retain managed JDK archives in cache
author: Khayalsan
date: 2026-10-18
branch: feat/vanilla-server-installation
touches:
  - README.md
  - src-tauri/src/java_runtime.rs
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/vanilla_install.rs
  - src/components/VanillaInstallPanel.tsx
---

## Decided
Managed JDKs live under runtimes/java and are reused from there. Their verified download archives are temporary for local and remote installation, then removed. On the next Java install or selection, remove the old cache/java folder in that configured root.

## Tried and ruled out
Keeping compressed JDK archives in cache/java duplicates the shared installed runtime and consumes disk without improving later launches.

## Watch out
Leave cache/minecraft intact. Resolve and validate the local or remote cache/java path before deleting the old managed folder. A remote archive belongs in the managed runtime parent only while uploading and extracting; remove it on completion or failure.
