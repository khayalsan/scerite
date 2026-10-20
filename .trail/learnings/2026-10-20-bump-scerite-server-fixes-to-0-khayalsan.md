---
title: Bump Scerite server fixes to 0.4.1
author: Khayalsan
date: 2026-10-20
branch: fix/server-port-conflicts
touches:
  - package-lock.json
  - package.json
  - src-tauri/Cargo.lock
  - src-tauri/Cargo.toml
  - src-tauri/tauri.conf.json
  - src-tauri/icons/logo.svg
  - CHANGELOG.md
---

## Decided
- The server port and player display fixes will be released as Scerite 0.4.1 after their branch merges. Keep package.json, the root package-lock entries, the Scerite Cargo package and lock entry, and tauri.conf.json on 0.4.1 together.
- Keep CHANGELOG.md entries under Unreleased on the feature branch; the merge-gated release preparation moves them into the versioned section.
## Tried and ruled out
- Do not keep historical 0.2.0 or 0.3.1 version constraints from old branch learnings; main is already 0.4.0 and this patch bump is intentional.
## Watch out
- Trail check currently reports lexical matches in remote_ssh.rs and server_port.rs from this branch. Those matches do not establish the ruled-out remote polling, recursive file reads, or arbitrary writes; review the code and commit rationale rather than treating the matches as behavior changes.
