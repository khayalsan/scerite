---
title: Vanilla server jar installation
author: Khayalsan
date: 2026-10-17
branch: feat/vanilla-server-installation
touches:
  - src-tauri/src/vanilla_install.rs src-tauri/src/remote_ssh.rs src-tauri/Cargo.toml
---

## Decided
Use Mojang's release manifest and metadata, verify size and SHA-1, cache under each configured Minecraft root, and copy only into a direct child of root/servers. Preserve an existing server.jar and require a stopped server. Remote transfer uses SFTP over the established SSH connection.
## Tried and ruled out
Reqwest's rustls feature pulled in AWS-LC and required extra Windows build tools here. rustls-no-provider with the Ring provider avoids that dependency. Remote curl was avoided because it would require download access from each server.
## Watch out
A remote Linux host needs sha1sum, cp, and ln. Check canonical paths after creating cache and server folders; do not trust symlinks or profile paths. Trail check's lexical matches around remote writes and OpenSSL must be judged against the actual stopped-state and dependency paths.
