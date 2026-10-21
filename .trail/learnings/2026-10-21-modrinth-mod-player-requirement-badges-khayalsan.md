---
title: Modrinth mod player requirement badges
author: Khayalsan
date: 2026-10-21
branch: feat/mod-client-requirements
touches:
  - CHANGELOG.md
  - src-tauri/src/modrinth.rs
  - src/App.css
  - src/components/PackagesPanel.tsx
---

## Decided
Mod badges in search use Modrinth's environment array only when all reported environments map to one player requirement. The selected release uses its own environment value because a project's versions can differ. Required maps to client_and_server; Optional maps to server_only_client_optional or client_or_server_prefers_both; Server maps to server_only, dedicated_server_only, or client_or_server.
## Tried and ruled out
The deprecated client_side and server_side fields can misdescribe the newer environment combinations, so use environment. Do not classify an ambiguous project-wide environment array as one requirement.
## Watch out
A project can have mixed release environments; its search card may have no badge until a compatible release is selected. Do not infer a requirement for unknown metadata. These badges describe Modrinth metadata, not a live check of what clients have installed.
