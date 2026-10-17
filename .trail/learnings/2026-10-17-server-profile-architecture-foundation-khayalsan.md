---
title: Server profile architecture foundation
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
- Keep server profile models, Tauri command adapters, and JSON persistence in separate Rust modules.
- Keep the frontend invoke calls behind `src/lib/serverProfiles.ts`.
- Store profile metadata in the Tauri app data directory; remote credentials are not part of the profile model.
- Start with profile management before adding process lifecycle, SSH, SFTP, or tmux operations.

## Tried and ruled out
- No alternative profile storage dependency was needed for the initial JSON-backed store.

## Watch out
- Validate and constrain local paths before any future filesystem operation.
- Future SSH work must verify host keys and store credentials in the operating system secure store.
- Do not imply that remote profiles can connect yet; remote transport is not implemented.
