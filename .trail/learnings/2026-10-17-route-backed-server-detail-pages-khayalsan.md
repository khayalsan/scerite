---
title: Route-backed server detail pages
author: Khayalsan
date: 2026-10-17
branch: feat/server-detail-pages
touches:
  - src/App.css
  - src/App.tsx
  - src/main.tsx
---

## Decided
Use HashRouter paths for server detail pages. Keep profile rows mounted and hide the non-selected rows so opening a server preserves file editor drafts and console state.

## Tried and ruled out
Filtering the profile map unmounts LocalFilesPanel and can discard unsaved edits before its close confirmation runs.

## Watch out
The detail heading may show a remote SSH address as context. This is display-only and does not use the local folder picker or select a path on the remote host.
