---
title: Velocity 4.3 sample forced hosts break minimal proxy config
author: Khayalsan
date: 2026-10-25
branch: feat/world-upload-cpu-velocity
touches:
  - CHANGELOG.md
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/velocity_download.rs
  - src/App.css
  - src/components/VelocityOnDemandPanel.tsx
---

## Decided
- Velocity 4.3.0 treats a missing [forced-hosts] table as its sample lobby/factions/minigames mappings. A managed proxy with only a backend server must write an explicit empty [forced-hosts] table; otherwise startup rejects those default mappings and never binds the public port.
- Keep the proxy tmux pane after exit and return bounded startup log lines on setup failure. Show phase updates in the settings UI so Java and proxy download delays are understandable.
## Tried and ruled out
- A minimal velocity.toml with only [servers] backend and no [forced-hosts] fails validation, even though the sample hosts were never intended for Scerite.
## Watch out
- On a failed setup, capture diagnostics before killing the proxy session and restoring backend config. Do not expose forwarding secrets or tokens in UI errors. Align the Velocity card with the centered server profile form.
