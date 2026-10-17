---
title: Trail check wording around profile storage
author: Khayalsan
date: 2026-10-17
branch: feat/server-profile-foundation
touches:
  - AGENTS.md
  - opencode.json
---

## Decided
- Keep the first server profile store JSON-backed with serde_json; broad references to storage/profile fields in general repo guidance do not propose a different dependency or database.

## Tried and ruled out
- No changes to the established profile storage were needed to satisfy the new repo guidance.

## Watch out
- `trail check` may flag generic architecture wording against the existing profile implementation by keyword; inspect the actual statements before treating those as conflicts.
