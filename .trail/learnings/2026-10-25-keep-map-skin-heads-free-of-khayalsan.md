---
title: Keep map skin heads free of silhouette overlays
author: Khayalsan
date: 2026-10-25
branch: fix/map-marker-silhouette
touches:
  - CHANGELOG.md
  - src/components/PlayerMapPanel.tsx
---

## Decided
- Map player markers show the fetched skin head without a person silhouette beneath it.
- Keep the existing offline greyscale styling and marker label.

## Tried and ruled out
- A UserRound fallback underneath a translucent offline skin head remains visible through the image.

## Watch out
- The Players detail page has a separate UserRound fallback. This change applies only to Map markers.
