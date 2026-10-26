---
title: Managed Playit tunnel per remote environment
author: Khayalsan
date: 2026-10-26
branch: feat/connection-method-navigation
touches:
  - CHANGELOG.md
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src/App.css
  - src/components/ConnectionMethodPanel.tsx
  - src/components/PlayitSetupPanel.tsx
---

## Decided
Scerite manages one opt-in Playit agent per remote environment under the managed runtimes directory. It downloads pinned official Linux daemon and CLI binaries, verifies their SHA-256 hashes, and runs the daemon and account-claim setup in tmux. Keep the account secret on the remote host and expose only a validated claim URL and status to the UI. Reuse installed binaries and the secret on restart. The tunnel's local target is loopback plus the server's listening port, or Velocity's public port when enabled.
## Tried and ruled out
A per-server plugin would depend on the Minecraft loader and would stop with the backend. A generic server agent or extra management port is unnecessary; Playit itself is the requested opt-in tunnel service. Downloading at every restart adds needless network dependence. Remote curl-pipe-shell installation would not verify a pinned binary.
## Watch out
The shared agent serves every server in its environment, so turning it off disconnects all of their tunnels. Claim setup needs the daemon socket. Do not return the secret file or full tmux output to the UI; extract only the claim URL.
