---
title: Theme selector uses only outer border
author: Khayalsan
date: 2026-10-22
branch: fix/theme-trigger-border
touches:
  - src/App.css
---

## Decided
The theme selector should have one visible outer border around its setting row. Remove the nested SelectTrigger border and let the wrapper supply keyboard focus indication.
## Tried and ruled out
Setting border-color: transparent on .theme-select-trigger did not work because the later .property-select-trigger rule overrides it.
## Watch out
SelectTrigger always includes property-select-trigger; use the more specific .theme-picker .theme-select-trigger override without changing other selects.
