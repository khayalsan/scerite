---
title: Per-server Java RAM and console TPS
author: Khayalsan
date: 2026-10-21
branch: feat/server-ram-tps
touches:
  - CHANGELOG.md
  - src/App.tsx
  - src/components/ServerOverviewPanel.tsx
---

## Decided
Expose per-server maximum Java heap as a numeric MB field in the existing profile editor. Read existing -XmxM/-XmxG flags into that field and write one -Xmx flag before the other launch arguments on save. Keep custom script commands untouched. Show one-minute TPS on the Server tab by using the existing managed console on Paper-compatible servers, refreshing only while that tab is open.

## Tried and ruled out
Minecraft's status ping does not provide TPS. Do not infer TPS from CPU usage or display a fabricated value for vanilla or mod loaders. Do not poll the entire remote console continuously when the Server tab is closed. Do not keep both a free-form -Xmx flag and the new RAM control, since they could conflict.

## Watch out
The tps console command may be missing or its output format may differ; the UI then shows Unavailable. Remote pane output needs a before/after comparison to avoid reusing an old TPS line. Saving RAM changes a future launch only. Keep -Xmx at least as large as -Xms. Existing Java installers preserve -X flags in profile launch arguments.
