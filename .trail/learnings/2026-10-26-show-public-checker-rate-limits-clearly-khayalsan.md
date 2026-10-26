---
title: Show public checker rate limits clearly
author: Khayalsan
date: 2026-10-26
branch: feat/public-reachability-check
touches:
  - src-tauri/src/public_reachability.rs
  - src-tauri/src/remote_ssh.rs
  - src/components/ServerOverviewPanel.tsx
---

## Decided
Map HTTP 429 from the public port checker to a short retry-later error for both local reqwest requests and remote curl/wget requests. A rate limit is an endpoint response, not a closed Minecraft port or SSH failure.
## Tried and ruled out
Passing curl's raw exit 22 and stderr through to the Server tab makes a rate limit look like a generic connection failure.
## Watch out
Remote curl reports 429 in stderr, while wget needs response headers enabled to expose the status. Checker errors must leave the SSH session usable. Unrelated Trail matches in this branch concern old Discord, EULA, file, and status work.
