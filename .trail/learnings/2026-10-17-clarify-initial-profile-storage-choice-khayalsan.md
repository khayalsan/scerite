---
title: Clarify initial profile storage choice
author: Khayalsan
date: 2026-10-17
branch: feat/server-profile-foundation
touches:
  - AGENTS.md
  - README.md
  - src-tauri/Cargo.lock
  - src-tauri/Cargo.toml
  - src-tauri/src/lib.rs
  - src/App.css
  - src/App.tsx
  - tsconfig.json
  - PRODUCT.md
  - opencode.json
---

## Decided
- The initial profile persistence uses the serde_json dependency already present in the project and stores one profile file in the Tauri app data directory. This is consistent with avoiding a new storage dependency for the first implementation.

## Tried and ruled out
- A separate database or storage plugin is not required for the current profile fields and CRUD scope.

## Watch out
- Revisit storage only when a concrete feature requires transactions, concurrent access, or substantially more data.
