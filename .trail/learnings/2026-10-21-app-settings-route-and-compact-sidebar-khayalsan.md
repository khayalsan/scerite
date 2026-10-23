---
title: App settings route and compact sidebar navigation
author: Khayalsan
date: 2026-10-21
branch: feat/app-settings-navigation
touches:
  - CHANGELOG.md
  - src/App.css
  - src/App.tsx
---

## Decided
App settings are a /settings route reached from a bottom sidebar link, separate from per-server Settings. Theme, EULA, and local Minecraft root controls live there. The Servers workspace no longer caps its width, so header actions and panels can align with the available right edge.
## Tried and ruled out
Keeping the theme picker in the Servers header leaves an app preference mixed with server actions. The old .theme-select-trigger background rule lost to the later .property-select-trigger rule; use a more specific selector for a transparent trigger.
## Watch out
At widths below 800px, the old CSS hid every inactive main nav link. Once /settings exists, keep a visible Servers control or mobile users cannot navigate back. The in-app browser could not reach the local Vite port during this task, so visual QA remained source-based.
