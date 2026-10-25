---
title: Scerite managed Velocity on-demand backend setup
author: Khayalsan
date: 2026-10-25
branch: feat/world-upload-cpu-velocity
touches:
  - CHANGELOG.md
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src/App.css
  - src/App.tsx
  - src/components/ServerOverviewPanel.tsx
  - src-tauri/resources/scerite-velocity.jar
  - src-tauri/src/velocity_download.rs
  - src/components/VelocityOnDemandPanel.tsx
  - src/lib/velocityOnDemand.ts
---

## Decided
- For opt-in on-demand startup on remote Paper/Purpur, Scerite owns a small Velocity plugin. The proxy stays on the original public Minecraft port while the backend moves to a free loopback port with modern forwarding; authenticated proxy connections trigger the existing tmux backend session, and idle shutdown sends Minecraft's stop command.
- Keep this off by default. Only install Java 25, Velocity, plugin files, and forwarding settings after the user enables it.
## Tried and ruled out
- Velocity alone cannot start a stopped backend on player join or stop it after inactivity.
- A third-party auto-start plugin was rejected in favor of Scerite's own plugin.
## Watch out
- Paper's proxies.velocity keys must exist; start Paper/Purpur once first. Disable BungeeCord forwarding in spigot.yml. Preserve unrelated edits to server.properties and paper-global.yml when toggling off. Failures after backend settings change must restore the previous settings.
- Verify proxy startup with both a listening public port and the plugin's initialized log marker before saving the enabled manifest.
