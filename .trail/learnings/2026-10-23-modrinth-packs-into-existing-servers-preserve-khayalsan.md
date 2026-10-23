---
title: Modrinth packs into existing servers preserve paths
author: Khayalsan
date: 2026-10-23
branch: feat/modpack-install
touches:
  - CHANGELOG.md
  - src-tauri/src/lib.rs
  - src-tauri/src/modrinth.rs
  - src-tauri/src/remote_ssh.rs
  - src/App.css
  - src/components/PackagesPanel.tsx
  - src-tauri/src/modpacks.rs
---

## Decided
- Modrinth modpacks install into the selected stopped server, preserving every existing server path. Report skipped conflicts after install.
- Browse packs by the existing Minecraft version and loader. Read mrpack server files plus overrides, with server-overrides taking precedence within the pack.
- The Mods tab exposes Required, Optional, and Server filters for Modrinth search badges.
## Tried and ruled out
- Replacing an existing world, mod, configuration, or launcher file as part of pack install. The user chose additive installation.
- Running pack installation while the server is active. This risks inconsistent files.
## Watch out
- Downloaded pack files and archive overrides must have bounded size, safe paths, and verified checksums before writing.
- Remote installs use SFTP exclusive creation and must reject symlink parents; a failed transfer can leave a partial newly created file that should be reviewed before retrying.
- Modrinth search result environments summarize the project; individual versions can differ.
