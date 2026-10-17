---
title: Initial profile persistence uses existing serde_json
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
- The first server profile implementation persists JSON in the Tauri app data folder with the serde_json dependency already in Cargo.toml.
- This is the concrete storage decision for profile CRUD in the initial architecture phase.

## Tried and ruled out
- None.

## Watch out
- Reassess storage when profile data gains transaction or concurrency requirements.
