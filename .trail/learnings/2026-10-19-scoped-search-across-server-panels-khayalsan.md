---
title: Scoped search across server panels
author: Khayalsan
date: 2026-10-19
branch: feat/server-players-logs
touches:
  - CHANGELOG.md
  - src/App.css
  - src/App.tsx
  - src/components/LocalFilesPanel.tsx
  - src/components/LogsPanel.tsx
  - src/components/PackagesPanel.tsx
  - src/components/PlayersPanel.tsx
  - src/components/ServerPropertiesPanel.tsx
  - src/components/SearchField.tsx
---

## Decided
- Add one reusable SearchField for scoped, case-insensitive client-side search across server panels. Files search the current folder, Properties search names/descriptions, installed packages search title/filename/dependency, and player/access lists search their visible entries.
- Console search filters the current output lines. Logs search filters lines in the selected log. Both keep their underlying output intact, so clearing search restores it without new remote reads.
## Tried and ruled out
- Do not turn these fields into recursive remote file search or extra SSH polling; existing panel data is already available locally for filtering.
## Watch out
- Keep Files selection and Properties unsaved edits intact while filtering. Show a distinct no-matches state for nonempty lists. Online player status remains best effort from latest.log.
