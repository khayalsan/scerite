---
title: Capture UI event values before deferred state updates
author: Khayalsan
date: 2026-10-17
branch: feat/server-detail-pages
touches:
  - src/App.css
  - src/components/ServerPropertiesPanel.tsx
  - src/components/ui/select.tsx
---

## Decided
- Read event values synchronously in React handlers before passing them into a functional state updater. A deferred updater can run after event dispatch, when `event.currentTarget` has already been cleared.

## Tried and ruled out
- No new state-management dependency is needed; capture the checkbox or input value, then update existing component state.

## Watch out
- Apply the same rule to controlled switches, selects, and text/number inputs so a late read does not throw and unmount the screen.
