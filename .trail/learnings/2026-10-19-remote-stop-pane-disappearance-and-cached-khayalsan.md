---
title: Remote stop pane disappearance and cached status
author: Khayalsan
date: 2026-10-19
branch: feat/server-players-logs
touches:
  - src-tauri/src/remote_ssh.rs
---

## Decided
Remote Stop rechecks tmux after send-keys fails. If the pane has disappeared and no Java process is using the server folder, return stopped so the UI leaves its cached Running/Online state immediately.

## Tried and ruled out
Treating any tmux send-keys failure as a permanent stop error leaves the UI with its prior Running status until the next remote poll. Inferring offline solely from unavailable Minecraft status/CPU/RAM would mislabel servers when those probes fail independently.

## Watch out
The Server overview derives Online from the remote tmux status, while player and usage probes are separate. A screenshot with Online plus unavailable metrics does not alone prove the game is reachable. Existing monitor polls every four seconds; a pane can disappear between its status check and send-keys.
