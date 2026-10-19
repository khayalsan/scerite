---
title: Retain remote startup errors in tmux console
author: Khayalsan
date: 2026-10-19
branch: feat/release-changelog
touches:
  - src-tauri/src/remote_ssh.rs src/App.tsx README.md CHANGELOG.md
---

## Decided
Keep a remote server's tmux pane after its process exits and treat pane_dead=1 as stopped. Capture the retained pane so startup errors remain visible in Console; clear the old dead session on the next start. A remote start opens the Console tab.

## Tried and ruled out
Polling status and replacing the console with "Server is stopped" loses output from quick failures. tmux's default removal of exited panes also loses the evidence. A transient foreground SSH launch is inappropriate for a persistent server.

## Watch out
A retained dead pane is not a running server. Preserve the outside-tmux Java process check before starting or editing files. Existing Trail warnings about README and CHANGELOG language include older branch changes and prior decisions; review the actual code path before treating phrase matches as regressions.
