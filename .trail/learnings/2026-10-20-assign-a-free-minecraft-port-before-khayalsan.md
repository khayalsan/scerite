---
title: Assign a free Minecraft port before server launch
author: Khayalsan
date: 2026-10-20
branch: fix/server-port-conflicts
touches:
  - src-tauri/src/server_port.rs src-tauri/src/remote_ssh.rs src-tauri/src/server_sessions.rs src/components/ServerOverviewPanel.tsx
---

## Decided
- Before launching a stopped local or remote server, read server.properties (default 25565), check listening TCP ports on that machine, and change only an occupied Minecraft port to the next free port.
- The user chose automatic assignment. Preserve existing server.properties content and reject symlinks; remote port discovery reads Linux /proc/net/tcp and /proc/net/tcp6 without adding another service or package.
- Refresh the Server overview address when status changes so a newly assigned port becomes visible.

## Tried and ruled out
- Letting Minecraft fail to bind leaves a dead console and an unclear start result.
- Requiring ss would add an avoidable remote package dependency; Linux /proc already serves the existing process checks.

## Watch out
- Port checks happen before launch and cannot prevent another process from claiming the port in the small interval before Minecraft binds.
- A newly selected remote port may still need firewall configuration for outside players.
