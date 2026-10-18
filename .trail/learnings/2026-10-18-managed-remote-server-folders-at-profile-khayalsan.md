---
title: Managed remote server folders at profile creation
author: Khayalsan
date: 2026-10-18
branch: feat/managed-remote-server-folders
touches:
  - README.md
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/server_profiles/commands.rs
  - src-tauri/src/server_profiles/store.rs
  - src/App.css
  - src/App.tsx
  - src/lib/serverProfiles.ts
---

## Decided
New remote profiles use the environment Minecraft root plus servers/<kebab-case server name> by default. Create or reuse only direct child directories after checking their resolved paths over SSH. Keep an explicit existing-folder option and validate it over SSH.

## Tried and ruled out
Do not derive the folder solely in React or rely on installation to create it later; profile creation must resolve and save the actual remote path. Do not rename folders when a profile name changes.

## Watch out
Reject symbolic links in managed path components, and prevent two profiles in one environment from sharing the same folder. If profile persistence fails after remote mkdir, an empty remote directory may remain; never delete it automatically.
