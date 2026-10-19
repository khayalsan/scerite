---
title: Require explicit live tmux pane status
author: Khayalsan
date: 2026-10-19
branch: feat/server-players-logs
touches:
  - CHANGELOG.md
  - src-tauri/src/remote_ssh.rs
---

## Decided
A remote tmux pane is running only when pane_dead returns exactly 0. Empty or unexpected successful output cannot prove a live pane. If Stop receives tmux's missing-pane error, check for a Java process in the server folder and report stopped when none remains.

## Tried and ruled out
The earlier Stop-only recheck was insufficient: remote_is_running interpreted empty tmux stdout as running, so the recheck could repeat the same false positive and leave the UI showing Running.

## Watch out
The screenshot showed a missing-pane error while the UI still showed Running. No direct host diagnostics were available, so empty tmux output is a plausible cause from code inspection rather than a confirmed remote observation. Keep unexpected tmux output from becoming an online claim.
