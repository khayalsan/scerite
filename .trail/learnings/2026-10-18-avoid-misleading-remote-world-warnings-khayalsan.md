---
title: Avoid misleading remote world warnings
author: Khayalsan
date: 2026-10-18
branch: feat/vanilla-server-installation
touches:
  - src/components/WorldsPanel.tsx
---

## Decided
World discovery inspects server-root directories one at a time and reports an inspection failure only when it affects the configured active world folder. Other top-level folders may be inaccessible or exceed the file-list limit without making a found world unusable.

## Tried and ruled out
A blanket warning for any failed folder inspection was misleading on remote servers: unrelated folders triggered it even when the world was discovered. Concurrent SFTP scans also caused avoidable remote session churn because a failed operation invalidates the cached session.

## Watch out
Root listing failures still need an error. A custom world folder that fails inspection cannot be distinguished from another inaccessible folder without new backend metadata; do not claim the list is exhaustive.
