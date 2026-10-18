---
title: Clarify default-on EULA setting copy
author: Khayalsan
date: 2026-10-18
branch: feat/vanilla-server-installation
touches:
  - src/App.tsx
---

## Decided
The global Auto enable EULA option uses this exact description: "Turn off if you really wanna read the EULA and accept before running the server".

## Tried and ruled out
The earlier description omitted "Turn off" and "the" before EULA, so it did not explain what disabling the default-on option does.

## Watch out
Keep the setting enabled by default and retain the existing local and remote start behavior; this correction only changes the visible description.
