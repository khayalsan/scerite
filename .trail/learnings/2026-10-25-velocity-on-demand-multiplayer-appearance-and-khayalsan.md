---
title: Velocity on-demand multiplayer appearance and live idle delay
author: Khayalsan
date: 2026-10-25
branch: feat/world-upload-cpu-velocity
touches:
  - CHANGELOG.md
  - src-tauri/resources/scerite-velocity.jar
  - src-tauri/src/lib.rs
  - src-tauri/src/remote_ssh.rs
  - src-tauri/src/velocity_download.rs
  - src/App.css
  - src/components/VelocityOnDemandPanel.tsx
  - src/lib/velocityOnDemand.ts
  - velocity-plugin/README.md
  - velocity-plugin/src/main/java/dev/khayal/scerite/velocity/SceriteVelocityPlugin.java
---

## Decided
A managed Velocity proxy must answer multiplayer pings with the backend's saved MOTD and server-icon.png while the backend is stopped. Read those files on a bounded timer in the Scerite Velocity plugin, cache their parsed values, and apply them in ProxyPingEvent. Keep the idle delay in the plugin's config.properties and reload it on that timer so Scerite can change it without restarting the proxy.
## Tried and ruled out
Velocity ping passthrough cannot supply the backend's MOTD or icon when on-demand startup has stopped the backend. Reading the files on every client ping would make public status requests do disk work.
## Watch out
The desktop app bundles a compiled plugin JAR in src-tauri/resources, so rebuild it after changing Java source. Existing setups need the new plugin and serverDirectory config; toggling off and back on redeploys them. The proxy's generated velocity.toml needs an empty [forced-hosts] table or default sample hosts fail validation.
