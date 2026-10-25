---
title: Edit installed package configs with form and raw views
author: Khayalsan
date: 2026-10-25
branch: feat/package-config-editor
touches:
  - CHANGELOG.md
  - package-lock.json
  - package.json
  - src/App.css
  - src/components/PackagesPanel.tsx
  - src/components/PackageConfigPanel.tsx
  - src/lib/packageConfig.ts
---

## Decided
- Installed mods and plugins open a configuration panel backed by the existing safe server-file list/read/write commands.
- Offer form controls for scalar fields in JSON, YAML, TOML, and properties files; offer Raw view for every readable text config file.
- Discover plugin config folders by package title or JAR name, then allow browsing when names differ. Mods browse config/ with likely matching files first.
- Re-read a file before overwriting it and require the server to be stopped. YAML and properties edits preserve surrounding text; TOML form saves warn that comments and formatting may change.

## Tried and ruled out
- A generic raw editor alone does not provide the requested settings UI.
- The Node-oriented @iarna/toml parser pulled stream and eval code into the browser bundle; use browser-compatible smol-toml instead.

## Watch out
- Package config paths are not standardized. If automatic matching misses a plugin folder, the user can browse it. Unsupported or complex fields remain available in Raw view.
