---
title: Caller-IP public port checks for local and remote servers
author: Khayalsan
date: 2026-10-26
branch: feat/public-reachability-check
touches:
  - CHANGELOG.md
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src/App.css
  - src/components/ServerOverviewPanel.tsx
  - src-tauri/src/public_reachability.rs
---

## Decided
The public port checker uses the caller's IP. Scerite calls it locally for a local server and through SSH from the remote server host for a remote server. The Server tab makes the port test manual and states that TCP reachability does not prove Minecraft readiness.
## Tried and ruled out
Calling the checker from the desktop for remote servers would test the desktop's public IP. Treating checker HTTP or JSON failures as SSH failures would unnecessarily invalidate a working SSH session.
## Watch out
The remote host's outbound public IP may differ from the inbound address players use. The checker currently returns only reachable, so the app cannot compare those addresses. Remaining Trail check matches about Discord, EULA, and unrelated tabs do not apply to this port check.
