---
title: Show environment names without SSH login in navigation
author: Khayalsan
date: 2026-10-23
branch: feat/environment-name-labels
touches:
  - CHANGELOG.md
  - src/App.tsx
---

## Decided
- Identify remote environments by their saved name in the environment cards, selected server sidebar, and server environment picker.
- Keep SSH user, host, and port in the environment edit form, where they are needed to manage the connection.
- Keep the Minecraft server address in the Server overview.

## Tried and ruled out
- Repeating username@host:port beside environment names adds noise to navigation and list views.

## Watch out
- The environment card still shows its Minecraft root directory; this change removes the SSH login line only.
