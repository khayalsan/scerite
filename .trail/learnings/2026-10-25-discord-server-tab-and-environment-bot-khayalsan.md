---
title: Discord server tab and environment bot split
author: Khayalsan
date: 2026-10-25
branch: feat/discord-bridge
touches:
  - src/App.tsx
  - src/components/DiscordBridgeSettings.tsx
  - src/App.css
---

## Decided
The Discord bridge stays one bot per remote environment. App settings owns the bot token, enable switch, update action, and resource usage. Each remote server's Discord tab owns only its default channel, action toggles and channels, and chat template. Saving a server rebuilds the environment mapping from current profiles, replacing only that server's values and preserving the others.

## Tried and ruled out
Keeping all server channel forms on the App settings page duplicates the new server tab and obscures which server is being configured. A local-server Discord tab would expose controls for a bridge that currently runs only on remote environments.

## Watch out
The existing enable_discord_bridge command redeploys and briefly restarts the shared environment bot even for a single server settings edit; tell users this in the tab. Switching from a remote Discord tab to a local server must reset the active tab. The new tab needs its own scroll container in the fixed-height server detail shell.
