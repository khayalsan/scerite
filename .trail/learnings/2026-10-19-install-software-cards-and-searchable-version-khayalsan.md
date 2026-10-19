---
title: Install software cards and searchable version
author: Khayalsan
date: 2026-10-19
branch: feat/server-players-logs
touches:
  - CHANGELOG.md
  - src/App.css
  - src/components/ServerInstallPanel.tsx
  - src/components/ui/combobox.tsx
---

## Decided
- The Install tab software picker is a single-select radio-card grid because one install operation accepts one loader. Show Mod and Plugin badges from the existing packageSupportForSoftware mapping, including both badges for Arclight variants.
- Minecraft version uses a searchable Base UI Combobox styled in the project's shadcn pattern. It remains restricted to versions returned by list_vanilla_versions, and snapshot filtering still updates that list.
## Tried and ruled out
- Keeping the software dropdown hid compatibility until an option was opened. Adding a separate compatibility mapping would duplicate existing loader rules.
- A free-text version field could submit a version not in the published list.
## Watch out
- The combobox adds frontend bundle weight. Vite may warn when the main chunk exceeds 500 kB; consider code splitting only if this grows further or impacts startup.
