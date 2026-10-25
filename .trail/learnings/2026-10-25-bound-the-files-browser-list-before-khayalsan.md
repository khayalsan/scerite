---
title: Bound the Files browser list before enabling scroll
author: Khayalsan
date: 2026-10-25
branch: fix/files-list-scroll
touches:
  - CHANGELOG.md
  - src/App.css
  - src/components/LocalFilesPanel.tsx
---

## Decided
Give the Files browser a bounded layout and put folder entries in their own scrollable child. Keep actions and search visible while a long directory scrolls; keep the editor in the adjacent pane.
## Tried and ruled out
Making only the outer Files panel scroll is insufficient because the browser itself clips oversized content with overflow hidden.
## Watch out
Every overflow-y auto needs a constrained ancestor height or flex/grid min-height zero; otherwise the list can expand instead of scrolling. The editor textarea must fit inside the same bounded browser.
