---
title: Share local mod and plugin favourites across servers
author: Khayalsan
date: 2026-10-25
branch: feat/package-favorites
touches:
  - CHANGELOG.md
  - src/App.css
  - src/components/PackagesPanel.tsx
  - src/lib/packageFavorites.ts
---

## Decided
- Mod and plugin favourites are saved on the local device and shared across all server profiles.
- Show a heart on Modrinth results and installed JARs. Offer a Favourites filter for catalog and installed lists.
- Keep saved Modrinth project metadata so the Favourites view works without repeating a search; check compatible versions when selected.

## Tried and ruled out
- Filtering only the current Modrinth result page loses favourites as soon as a different query is run.
- Storing favourites in an individual server profile does not meet the shared-across-servers requirement.

## Watch out
- Manually added JARs have no Modrinth project ID. Their favourite key uses the JAR filename and they appear in installed favourites, not the Modrinth catalog.
