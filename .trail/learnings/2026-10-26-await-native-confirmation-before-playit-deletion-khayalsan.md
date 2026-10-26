---
title: Await native confirmation before Playit deletion
author: Khayalsan
date: 2026-10-26
branch: feat/connection-method-navigation
touches:
  - src/components/PlayitSetupPanel.tsx
---

## Decided
Playit deletion must await the Tauri native dialog's `ask` result before invoking the Rust delete command. Set the busy state before opening the dialog and return on Cancel. Match the existing mod/plugin removal confirmation pattern.
## Tried and ruled out
The browser `window.confirm` appeared to the user but did not reliably block the Tauri action; the Playit files were removed while the dialog was still awaiting their choice. Do not use it for this destructive flow.
## Watch out
Keep the backend delete invoke strictly after `await ask(...)` returns true. Cancel, dismiss, or dialog errors must not invoke deletion. This is a client confirmation; the Rust command remains a separate privileged action.
