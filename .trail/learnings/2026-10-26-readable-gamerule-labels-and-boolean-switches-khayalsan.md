---
title: Readable gamerule labels and boolean switches
author: Khayalsan
date: 2026-10-26
branch: fix/world-gamerules-layout
touches:
  - CHANGELOG.md
  - src/App.css
  - src/components/WorldsPanel.tsx
---

## Decided
In the Worlds tab, display gamerule names without the minecraft: namespace and turn snake_case or camelCase IDs into readable labels. Boolean rules use the existing settings switch next to the label and save when toggled. Numeric rules retain an input and Apply button. Keep the original ID for commands and NBT writes.

## Tried and ruled out
A true/false dropdown with a separate Apply button required two interactions and made each rule card taller. Raw IDs such as minecraft:universal_anger were hard to scan.

## Watch out
Search should match both the readable label and raw ID. Revert an optimistic switch when the save or console send fails. Do not replace Minecraft rule IDs in persistence or console commands with display labels.
