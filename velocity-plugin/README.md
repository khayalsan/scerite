# Scerite Velocity plugin

This plugin starts a managed Paper or Purpur backend after an authenticated player connects to Velocity, then stops the backend after the idle delay chosen in Scerite. It reads the backend's MOTD and server icon for Velocity's multiplayer listing. It uses the existing tmux session and a loopback backend port. No additional management service or port is needed.

Build with JDK 25 and Gradle from this directory:

```sh
gradle jar
```

Copy `build/libs/scerite-velocity.jar` to `src-tauri/resources/scerite-velocity.jar` before building the desktop app. The bundled JAR is loaded from that path at compile time.
