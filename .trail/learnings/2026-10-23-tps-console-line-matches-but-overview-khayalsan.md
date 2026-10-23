---
title: TPS console line matches but overview misses it
author: Khayalsan
date: 2026-10-23
branch: dev
touches:
  - CHANGELOG.md
  - src/components/ServerOverviewPanel.tsx
---

## Decided
- A console response such as `[11:06:57 INFO]: TPS from last 1m, 5m, 15m: 20.0, 20.0, 20.0` is valid. The Server tab must keep a moving console cursor and accumulate fresh lines while awaiting `tps` so remote pane redraws and split responses do not discard it.
## Tried and ruled out
- Changing only the TPS regex: the reported line already matched it, so this alone cannot fix the unavailable reading.
## Watch out
- A successful frontend build does not prove the status reading works against a live remote tmux session; verify in a fresh development build.
