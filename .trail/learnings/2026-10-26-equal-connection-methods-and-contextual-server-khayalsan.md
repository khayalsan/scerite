---
title: Equal connection methods and contextual Server subtabs
author: Khayalsan
date: 2026-10-26
branch: feat/connection-method-navigation
touches:
  - CHANGELOG.md
  - src/App.css
  - src/App.tsx
  - src/components/ServerOverviewPanel.tsx
  - src/components/ConnectionMethodPanel.tsx
---

## Decided
The Server overview presents Direct, Tailscale, and Playit as equal selectable banners. Once a method is open, show all three as nested links under Server; hide those links outside the Server section. Keep the existing public port check under Direct and preserve its local-versus-remote request routing.

## Tried and ruled out
A fallback sequence would imply one connection method should be attempted only after another fails. The user wants to choose by goals and comfort. Permanent sidebar links would clutter unrelated server sections.

## Watch out
When navigating away from Server or changing profiles, clear the active connection method. If Velocity is enabled, use its public port; if proxy status cannot be read, do not suggest a backend port as the public address.
