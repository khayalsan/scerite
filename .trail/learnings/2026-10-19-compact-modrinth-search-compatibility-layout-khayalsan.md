---
title: Compact Modrinth search compatibility layout
author: Khayalsan
date: 2026-10-19
branch: feat/server-players-logs
touches:
  - CHANGELOG.md
  - src/App.css
  - src/components/PackagesPanel.tsx
---

## Decided
In the shared Mods and Plugins PackagesPanel, keep the search field first and place Sort results, detected Minecraft version, and known server loader together below it. Render detected compatibility as compact read-only text, not input-like boxes. Remove the redundant catalog introduction and filter explanation.

## Tried and ruled out
Removing the loader control unconditionally would break Modrinth search for profiles whose software cannot be identified. Keep the existing loader selector only for that fallback case, alongside the compact compatibility row.

## Watch out
PackagesPanel serves both tabs. Preserve the version-detection error and Retry action when moving version information; search still needs a known Minecraft version. The row should wrap on narrow widths.
