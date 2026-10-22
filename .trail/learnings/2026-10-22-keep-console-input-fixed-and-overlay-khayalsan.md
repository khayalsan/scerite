---
title: Keep console input fixed and overlay suggestions above
author: Khayalsan
date: 2026-10-22
branch: feat/console-autocomplete
touches:
  - CHANGELOG.md
  - src/App.css
  - src/App.tsx
  - src/components/ConsoleLogLine.tsx
---

## Decided
Keep the console command field fixed at the bottom. Autocomplete suggestions overlay the console immediately above that field so typing does not move the input. Color timestamps, warning lines, error lines, and standard player chat differently; provide a chat-only switch in the console toolbar.
## Tried and ruled out
Putting suggestions below the input in normal document flow moved the input up and down while typing. That behavior was uncomfortable and the user corrected it.
## Watch out
Chat-only filtering recognizes standard Minecraft log messages beginning with <player> after the log prefix; plugin-specific chat formats may differ. Apply search after the chat filter and leave the underlying console output unchanged.
