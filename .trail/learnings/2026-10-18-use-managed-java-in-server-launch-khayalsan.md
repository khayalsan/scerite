---
title: Use managed Java in server launch commands
author: Khayalsan
date: 2026-10-18
branch: feat/vanilla-server-installation
touches:
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/server_profiles/model.rs
  - src-tauri/src/server_sessions.rs
  - src-tauri/src/vanilla_install.rs
  - src/App.tsx
  - src/components/VanillaInstallPanel.tsx
  - src/lib/serverProfiles.ts
---

## Decided
Managed JDK installation records its Java executable on the profile. Custom launch commands keep their executable and receive JAVA_HOME plus a PATH prefixed with the managed JDK bin. Existing installations can configure Java for a selected Minecraft version without touching the server jar.

## Tried and ruled out
Replacing every custom launch executable with the managed java binary would break scripts and their launch arguments. Merely downloading the JDK leaves scripts using system java.

## Watch out
A detached local session manager from an older build cannot apply the new launch environment; check its protocol before sending a managed-Java start request. Keep remote execution in tmux and validate that the Java executable resolves inside the configured runtime root.
