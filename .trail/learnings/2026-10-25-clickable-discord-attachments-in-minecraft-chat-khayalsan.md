---
title: Clickable Discord attachments in Minecraft chat
author: Khayalsan
date: 2026-10-25
branch: feat/discord-bridge
touches:
  - src-tauri/src/discord_bridge.ts
  - src-tauri/src/remote_ssh.rs
---

## Decided
Discord attachments relay into Minecraft as short clickable labels (`[image]`, `[video]`, `[audio]`, or `[file]`) using the attachment proxy URL. Messages that contain only attachments must also relay. Java 1.21.5+ uses `click_event` with `url`; older saved Minecraft versions use `clickEvent` with `value`.

## Tried and ruled out
A literal `[image](long signed URL)` string would clutter chat and would not make only the label clickable. A single click-event format does not cover the Java 1.21.5 text-component change.

## Watch out
Keep URLs as data inside JSON.stringify, accept only HTTPS, and never send them through a shell command. Discord media URLs can expire; the bot uses the URL supplied on the message. If a profile has no recorded Minecraft version, the bridge assumes the current click-event format.
