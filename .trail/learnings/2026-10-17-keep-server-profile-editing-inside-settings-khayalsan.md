---
title: Keep server profile editing inside Settings
author: Khayalsan
date: 2026-10-17
branch: feat/server-detail-pages
touches:
  - src/App.css
  - src/App.tsx
---

## Decided
- In server details, open the shared profile editor inside the Settings tab so it replaces the settings summary; keep the edit region scrollable within the bounded workspace.

## Tried and ruled out
- Rendering the edit form at the workspace root makes it feel detached from Settings and can clip long forms in the fixed-height detail shell.

## Watch out
- `.server-detail-shell .workspace` uses `overflow: hidden`; detail tab content must own `min-height: 0` and `overflow-y: auto` for long settings and edit forms.
