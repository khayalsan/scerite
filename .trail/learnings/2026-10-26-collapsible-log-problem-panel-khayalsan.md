---
title: Collapsible log problem panel
author: Khayalsan
date: 2026-10-26
branch: feat/log-problem-suggestions
touches:
  - src/App.css
  - src/components/LogsPanel.tsx
---

## Decided
The detected problems panel uses native details/summary so it starts open and can be collapsed with a keyboard or pointer.
## Tried and ruled out
The suggestions remain read-only; cached log text is never used as authority for server actions.
## Watch out
Trail check matches unrelated popup variables and Modrinth favourite filtering in the Logs panel. These do not apply to log excerpts or the panel's local text filter.
