---
title: Handle disappearing tmux pane during console polling
author: Khayalsan
date: 2026-10-19
branch: feat/release-changelog
touches:
  - src-tauri/src/remote_ssh.rs src/App.tsx CHANGELOG.md
---

## Decided
Treat tmux's "can't find pane/window/session" during remote console capture as an empty snapshot. Console capture is separate from the status read and can race with start replacing a pane or an exiting process. Keep polling when the server is active; let the next snapshot show output.

## Tried and ruled out
Marking SSH unreachable for every console read error misreports a tmux pane race as a connection failure and stops the console refresh.

## Watch out
Do not suppress unrelated tmux errors; preserve their diagnostic message. Trail check still reports phrase matches from older changes on this branch, not this console race fix.
