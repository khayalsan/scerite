---
title: Trigger desktop releases on merged pull requests only
author: Khayalsan
date: 2026-10-19
branch: fix/release-on-pr-merge
touches:
  - ".github/workflows/release.yml"
---

## Decided
- Desktop releases remain automatic, but start only when a pull request into main is merged. A direct push to main must not start a release.
- Use pull_request_target closed with an explicit merged condition. The repository is private, and the release needs signing secrets plus a write token. Checkout the merge_commit_sha, which is already on main after a successful merge, rather than the PR head.

## Tried and ruled out
- Manual dispatch and Markdown-only path filters did not match the requested behavior. Their local unpushed branches were discarded.

## Watch out
- A merged PR that only changes documentation still triggers a release; the rule is based on merge method, not file type.
- origin/main and remote tags reached v0.3.2 after two README pushes on 2026-10-19. If the release should be numbered 0.3.1, reconcile the unpublished v0.3.1 draft, the v0.3.1/v0.3.2 remote tags, and main's version files before merging the release PR. Otherwise prepare-release.mjs selects the next patch.
