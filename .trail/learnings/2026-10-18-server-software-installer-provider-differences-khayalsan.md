---
title: Server software installer provider differences
author: Khayalsan
date: 2026-10-18
branch: feat/server-software-installation
touches:
  - README.md
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/vanilla_install.rs
  - src/App.tsx
  - src/components/VanillaInstallPanel.tsx
  - src-tauri/src/software_install.rs
  - src/components/ServerInstallPanel.tsx
---

## Decided
Server software installation uses the existing Minecraft roots and server profiles. Direct jars use per-software/version/build caches; Forge, NeoForge, and Quilt run their upstream installers only in empty server folders. Arclight exposes separate Forge, NeoForge, and Fabric variants. Spigot BuildTools and mod/plugin packages are later work.

## Tried and ruled out
Quilt installer metadata has no stable flag, so select its first published installer and verify its published SHA-256. Do not treat all loaders as direct runnable jars or force a custom launch executable to managed Java.

## Watch out
The vanilla version list includes snapshots, but each provider may not publish every Minecraft version. Forge versions before 1.17 have a different launch format. Remote installer execution requires SSH/SFTP plus standard Linux commands; a failed upstream installer can leave partial files in the empty target folder.
