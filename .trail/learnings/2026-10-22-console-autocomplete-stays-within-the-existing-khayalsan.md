---
title: Console autocomplete stays within the existing console
author: Khayalsan
date: 2026-10-22
branch: feat/console-autocomplete
touches:
  - CHANGELOG.md
  - src/App.css
  - src/App.tsx
  - src/components/ConsoleCommandInput.tsx
---

## Decided
Console autocomplete uses a short list of common Minecraft commands plus commands successfully sent during the current console session. Arrow keys select, Tab completes, and Enter sends the typed command. Suggestions render below the input and shrink the scrollable output area while open.
## Tried and ruled out
Placing suggestions in an overlay above the bottom-aligned console input did not match the requested downward list. Reading live command metadata would add remote commands and loader-specific parsing for this UI feature.
## Watch out
Keep autocomplete local to the connected console. Do not send a suggestion on Tab or change the backend command path. Installed plugin commands appear in recent suggestions after the user sends them once.
