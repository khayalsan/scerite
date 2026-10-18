---
title: Modrinth packages in server tabs
author: Khayalsan
date: 2026-10-18
branch: feat/mod-plugin-management
touches:
  - README.md
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src/App.css
  - src/App.tsx
  - src-tauri/src/modrinth.rs
  - src/components/PackagesPanel.tsx
---

## Decided
The server page has one Mods & plugins tab. It lists existing JARs from mods/ or plugins/ and searches Modrinth by user-selected Minecraft version and loader because the server profile does not persist installed software metadata. A selected release is verified against its published SHA-512 and installed only while the server is stopped, without replacing existing files. Local and remote installs share this contract.

## Tried and ruled out
Do not infer the loader from a jar filename or automatically download required dependencies without resolving their exact compatible versions. The tab reports required dependency counts for manual installation.

## Watch out
Modrinth project search and release listing use separate compatibility filters. Revalidate project type, version, loader, and server environment in Rust before downloading. Remote uploads use SFTP and verify the uploaded file before linking it into the package folder. The first catalog is Modrinth; manual JAR import and package removal are future work.
