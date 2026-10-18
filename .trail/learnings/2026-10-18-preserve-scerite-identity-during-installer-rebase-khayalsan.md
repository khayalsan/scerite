---
title: Preserve Scerite identity during installer rebase
author: Khayalsan
date: 2026-10-18
branch: feat/mod-plugin-management
touches:
  - src-tauri/Cargo.lock
  - src-tauri/src/java_runtime.rs
  - src-tauri/src/server_sessions.rs
  - src-tauri/src/vanilla_install.rs
---

## Decided
When rebasing the vanilla, loader, and Modrinth work over main's application rename, keep the Scerite package name, Tauri product name, bundle identifier, and library name while carrying the feature branch's 0.2.0 version.

## Tried and ruled out
Taking the feature side of every version conflict would restore the old ServerCrafter identity. Taking main's side unchanged would lose the requested 0.2.0 bump.

## Watch out
The replay also restored a few user-facing ServerCrafter strings and the Paper download User-Agent contact URL; update these to Scerite after resolving the manifest conflicts. Internal temporary-file prefixes can remain stable.
