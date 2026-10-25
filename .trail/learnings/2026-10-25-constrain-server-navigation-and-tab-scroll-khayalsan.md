---
title: Constrain server navigation and tab scroll regions
author: Khayalsan
date: 2026-10-25
branch: fix/server-navigation-scroll
touches:
  - src/App.css
---

## Decided
The server detail shell fixes the workspace at viewport height and hides overflow. To make long content reachable, give the vertical server tab list flex: 1, min-height: 0, and overflow-y: auto, while preserving the App settings link below it. Give the visible Server overview and Install panels flex: 1, min-height: 0, and their own vertical overflow. The main sidebar navigation also flexes so saved-server shortcuts can scroll.
## Tried and ruled out
Adding overflow-y: auto only to the content element can fail when its flex ancestors keep their intrinsic height. Scrolling the whole workspace would disrupt console and other tabs with their own scroll regions.
## Watch out
At narrow widths, the server tab list becomes horizontal; override the desktop flex and vertical overflow there so the mobile navigation stays a single horizontal strip.
