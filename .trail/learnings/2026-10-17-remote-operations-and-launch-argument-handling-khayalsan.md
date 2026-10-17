---
title: Remote operations and launch argument handling
author: Khayalsan
date: 2026-10-17
branch: feat/local-server-operations
touches:
  - src/App.tsx
---

## Decided
- Remote server launch commands run inside a detached tmux session, so SSH disconnects do not terminate the Minecraft process.
- Console input is sent to the tmux session, and remote launch arguments are passed as individually quoted argv values.
- Profile argument input accepts whitespace and newlines as separators; quotes group values containing spaces.

## Tried and ruled out
- Do not interpret Trail's phrase matches for SSH launch or console commands as proof that remote launch is a transient SSH foreground process or console input is a shell command; inspect the tmux command path.

## Watch out
- Trail check compares the complete branch diff and can flag lexical matches in older remote-management changes. Explain these known context matches in the PR while preserving host-key verification and tmux persistence.
