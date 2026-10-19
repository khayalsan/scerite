---
title: Install version combobox portal colors
author: Khayalsan
date: 2026-10-19
branch: feat/server-players-logs
touches:
  - src/App.css
---

## Decided
The Install tab version combobox opens through a portal outside .app-shell. Style the portaled popup with root theme variables such as --popover, --popover-foreground, and --border so the menu has an opaque surface in every theme.

## Tried and ruled out
Using --app-paper, --app-ink, and --app-line on the popup left those variables undefined outside .app-shell, making underlying controls visible through the menu.

## Watch out
The input group stays inside .app-shell and can continue using --app-* variables. Check the portal boundary when styling future overlays.
