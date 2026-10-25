---
title: Velocity MOTD fallback when backend omits motd
author: Khayalsan
date: 2026-10-25
branch: feat/world-upload-cpu-velocity
touches:
  - src-tauri/resources/scerite-velocity.jar
  - src-tauri/src/remote_ssh.rs
  - velocity-plugin/src/main/java/dev/khayal/scerite/velocity/SceriteVelocityPlugin.java
  - velocity-plugin/src/main/resources/velocity-plugin.json
---

## Decided
Velocity must always set the multiplayer description in ProxyPingEvent when mirroring a managed backend. When server.properties has no motd key, use Minecraft's default "A Minecraft Server" rather than leaving Velocity's default "A Velocity Server" in the ping.
## Tried and ruled out
Reading the icon successfully does not prove a MOTD key exists in the same directory. Setting the description only when getProperty("motd") is non-null lets Velocity's own description leak through.
## Watch out
The plugin JAR is bundled in src-tauri/resources and must be rebuilt. Mark existing deployed proxy configs as update required when changing the bundled plugin; a configured serverDirectory alone does not prove that the new Java code is installed.
