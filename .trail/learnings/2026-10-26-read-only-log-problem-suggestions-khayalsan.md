---
title: Read-only log problem suggestions
author: Khayalsan
date: 2026-10-26
branch: feat/log-problem-suggestions
touches:
  - src/App.css
  - src/components/LogsPanel.tsx
  - src/lib/logProblems.ts
---

## Decided
LogsPanel detects a small set of explicit server errors from the loaded log excerpt and shows read-only suggestions with links to matching lines.
## Tried and ruled out
Automatic fixes from log text are unsafe because a generic error line does not establish the cause or an authorized action.
## Watch out
Trail check matched unrelated learnings about theme trigger borders, Modrinth favourites, and popup CSS variables. The log panel changes do not touch those behaviors.
