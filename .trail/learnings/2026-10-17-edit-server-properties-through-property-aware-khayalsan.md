---
title: Edit server.properties through property-aware controls
author: Khayalsan
date: 2026-10-17
branch: feat/server-detail-pages
touches:
  - src/App.css
  - src/App.tsx
  - src/components/ServerPropertiesPanel.tsx
---

## Decided
- Edit `server.properties` through the existing `serverFiles` read/write interface. Preserve comments, unknown entries, original newline style, and untouched bare keys; use controls based on property types and confirm before saving. Restrict writes to stopped servers.

## Tried and ruled out
- A generic raw text editor does not make common boolean and numeric settings easier to edit, so use switches, selects, and typed inputs for known settings.

## Watch out
- `server.properties` may not exist until the server has started once. Keep load and save errors actionable, and keep save disabled while the server is running.
