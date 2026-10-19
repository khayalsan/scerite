---
title: Modrinth plugin search cannot use environment facet
author: Khayalsan
date: 2026-10-19
branch: fix/plugin-search-environment
touches:
  - src-tauri/src/modrinth.rs
---

## Decided
- Plugin search omits the Modrinth environment facet while retaining project type, loader, and Minecraft version facets. Mod search keeps the environment facet.
- Version listing and install-plan validation still check server compatibility before a plugin can be installed.

## Tried and ruled out
- Filtering by all_project_types alone did not fix plugin search. For Paper 1.21.11 and Ess, the full search facet set returned zero hits; removing only the environment facet returned 146, with all 40 first-page hits surviving the app's filters.
- Including server-capable environment values excludes these projects because Modrinth search hits report environment unknown or empty even while deprecated server_side reports required.

## Watch out
- A release build started before this correction was cancelled while still unpublished. Delete its v0.3.1 draft and tag before merging this fix if the corrected build must keep version 0.3.1.
