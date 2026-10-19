---
title: Confirm managed package removal before deleting
author: Khayalsan
date: 2026-10-19
branch: feat/server-players-logs
touches:
  - src/components/PackagesPanel.tsx CHANGELOG.md
---

## Decided
- Use Tauri's native ask dialog for managed mod and plugin removal. Only an explicit Remove response may call a destructive Tauri command.
- The optional unused-dependency choice must name its negative action Keep dependencies; it still removes the selected mod after the prior Remove mod confirmation.
- Set the busy state only when deletion begins, and guard against overlapping removal dialogs.

## Tried and ruled out
- Browser window.confirm was used by both removal paths and did not reliably honor Cancel in the reported desktop flow.
- Labeling the dependency choice Cancel was misleading because the selected mod still gets removed.

## Watch out
- Previewing a mod removal is read-only. Keep both remove_modrinth_mod and remove_modrinth_plugin behind explicit confirmation.
