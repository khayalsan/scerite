---
title: Installed package sizes and short Modrinth searches
author: Khayalsan
date: 2026-10-19
branch: fix/package-sizes-short-search
touches:
  - src-tauri/src/modrinth.rs
  - src/components/PackagesPanel.tsx
---

## Decided
Installed mod and plugin rows use byte values and should display B below 1 KiB, KB below 1 MiB, and MB otherwise, matching the Files panel. Modrinth search permits empty and one-character queries for both mods and plugins; the 100-character maximum remains.

## Tried and ruled out
Trail check interpreted a change to the long installed package row as reintroducing automatic dependency download. The row's dependency text and behavior were already present; this edit only changes size formatting.

## Watch out
PackagesPanel renders installed rows for both mods and plugins. Its search input and the Rust search_modrinth command must allow the same query lengths.
