---
title: Update managed Velocity plugin without disabling proxy
author: Khayalsan
date: 2026-10-25
branch: feat/world-upload-cpu-velocity
touches:
  - CHANGELOG.md
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/velocity_download.rs
  - src/App.css
  - src/components/VelocityOnDemandPanel.tsx
  - src/lib/velocityOnDemand.ts
---

## Decided
An installed on-demand Velocity proxy needs an in-place Update button. Stop and restart the proxy with the existing managed Java runtime, replace only the bundled Scerite plugin JAR and its config marker, preserve the idle delay and backend settings, and restore the prior plugin files if startup fails.
## Tried and ruled out
Turning on-demand startup off and on is not a safe updater: disabling first restores Paper settings and can fail when Paper has rewritten proxies.velocity.secret after setup. Reinstalling the whole proxy would also unnecessarily rotate forwarding configuration and disrupt working backend settings.
## Watch out
Require the backend to be stopped before updating, derive the live proxy Java path from its tmux pane process and validate it is under the managed Java 25 runtime. Keep update status generic for future plugin versions. The old proxy JAR and config need to be backed up before the proxy stops so a failed update can recover.
