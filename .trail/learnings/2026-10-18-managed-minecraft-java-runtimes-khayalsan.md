---
title: Managed Minecraft Java runtimes
author: Khayalsan
date: 2026-10-18
branch: feat/vanilla-server-installation
touches:
  - src-tauri/src/java_runtime.rs src-tauri/src/vanilla_install.rs src-tauri/src/remote_ssh.rs
---

## Decided
Read javaVersion.majorVersion from Mojang version metadata instead of maintaining a version table. Select the matching Eclipse Temurin JDK from Adoptium's assets API. Store versioned builds under root/runtimes/java/<major>/<platform>/<build> and archives under root/cache/java. Install Java before server.jar so a Java failure leaves the server folder untouched. Update Java launch profiles to use the managed executable while preserving custom launch executables.
## Tried and ruled out
Hard-coded Minecraft-to-Java rules become stale. Relying on remote curl would require outbound access on every host; use the existing SFTP transport. Avoid the AWS-LC and vendored OpenSSL feature paths in the HTTP client.
## Watch out
Temurin downloads redirect from GitHub to GitHub asset hosts. Verify SHA-256 and archive paths before extraction, reject symlinked cache/runtime parents, and use separate build folders so existing runtimes are never overwritten. Remote Linux needs sha256sum and tar. Trail check can match words in TLS, SSH, or path code even when its prohibited behavior is absent; document actual checks in commits.
