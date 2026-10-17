---
title: Keep server settings focused on editing
author: Khayalsan
date: 2026-10-17
branch: feat/server-detail-pages
touches:
  - src/App.tsx
---

## Decided
- Server detail Settings should contain the profile editor directly, without a separate Edit/Remove summary. Keep the saved editor open after Save changes and allow switching tabs while it remains open.

## Tried and ruled out
- Closing the editor after save returns to a redundant summary and makes Settings feel like a separate mode that must be exited before using other tabs.

## Watch out
- Keep the editor mounted while its tab is hidden so unsaved drafts survive tab switches, and clear edit state when leaving the server detail page.
