---
title: Server icon picker has a separate file button
author: Khayalsan
date: 2026-10-22
branch: fix/server-icon-file-button
touches:
  - src/App.css
  - src/components/ServerIconControl.tsx
---

## Decided
Use a dedicated Choose image button for the server multiplayer icon picker. Keep the selected preview and filename separate, with Save icon as a distinct action.
## Tried and ruled out
A visible native file input makes its whole field the picker trigger, which was confusing in this setting.
## Watch out
Reset the hidden input value after selection so choosing the same file again still fires change. Keep image crop, upload, and stopped-server behavior unchanged.
