---
title: Server detail replaces the app navigation
author: Khayalsan
date: 2026-10-17
branch: feat/server-detail-pages
touches:
  - src/App.css
  - src/App.tsx
---

## Decided
On a server detail route, replace the app's Servers navigation rail with that server's status and Console, Files, and Settings tabs. Keep those controls in the shell's left rail and let the selected tab fill the workspace.

## Tried and ruled out
Nesting a second status and tab sidebar inside the server card leaves the original Servers navigation visible and makes the page feel like a panel. The detail route should switch the outer app shell sidebar instead.

## Watch out
Keep the saved-server list shell unchanged. At narrow widths, move the detail status above a horizontally scrollable tab row rather than reusing the fixed Servers bottom bar.
