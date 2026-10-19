---
title: Modrinth plugin search uses all project types
author: Khayalsan
date: 2026-10-19
branch: fix/plugin-search
touches:
  - src-tauri/src/modrinth.rs
---

## Decided
Plugin search uses Modrinth search hits' all_project_types field to identify plugin projects. The project detail endpoint still reports plugin projects such as EssentialsX as project_type mod, so install-plan validation accepts that legacy type while checking the selected release's server-compatible loader and Minecraft version.

## Tried and ruled out
The exact Paper 1.21.11 Ess search returned 146 hits from Modrinth, so the empty UI result was not an empty catalog. Search facets alone were not enough: the app's post-response project_type comparison removed every plugin hit. Changing only search would still leave install-plan validation rejecting plugins.

## Watch out
Modrinth's search hit has all_project_types, but project detail does not. Preserve the separate version and loader checks during installation. Live API responses can drift; verify the fields again if Modrinth changes its project schema.
