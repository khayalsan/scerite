---
title: Playit release asset redirects and checksums
author: Khayalsan
date: 2026-10-26
branch: feat/connection-method-navigation
touches:
  - src-tauri/src/remote_ssh.rs
---

## Decided
Playit GitHub release downloads need a dedicated reqwest client that follows only HTTPS redirects to github.com or release-assets.githubusercontent.com, with a bounded redirect count. Keep the shared vanilla installer client unchanged because it intentionally disables redirects. Retain pinned SHA-256 verification after following the redirect.
## Tried and ruled out
The published v1.0.10 checksums were correct. Replacing hashes or disabling verification would hide the actual bug: reqwest read GitHub's 302 response body because the shared client uses Policy::none. A direct independent download matched the published hash.
## Watch out
GitHub serves the release binaries from release-assets.githubusercontent.com. A 302 is not an HTTP error for error_for_status, so a no-redirect client reaches the checksum comparison with redirect content. Test both daemon and CLI assets for amd64 and aarch64 through the real download path.
