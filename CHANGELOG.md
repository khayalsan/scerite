# Changelog

Add release notes under **Unreleased** before merging a pull request into `main`. The release workflow moves them into the new version and publishes them with the GitHub release.

## [Unreleased]

### Added

- Remote environments can opt in to a user-owned Discord bot without choosing a channel immediately. Chat is two-way by default, with separate direction controls. Online, offline, joins, leaves, player deaths (including PvP), and advancements can each use their own channel or share the server's default channel. It is off by default and installs on the remote host only when enabled.
- Discord settings show when the remote bot needs an update and can redeploy it without re-entering the token or channels. Minecraft chat appears in Discord under each player's name and skin head when webhooks are available; joins, leaves, deaths, and advancements use embeds.
- Installed mods and plugins now open a configuration panel. JSON, YAML, TOML, and properties files have form controls, with a Raw view for other files or precise edits.
- Mods and plugins can be favourited with a heart. Favourites are saved on this device across servers and can be filtered in search results and installed lists.
- The Mods tab can browse and install compatible Modrinth modpacks into a stopped server. Existing server files are preserved, and the install summary lists conflicting paths that were skipped. Mod search results show Required, Optional, and Server badges, which can also filter results.
- A Map tab plots online players on an interactive, chunk-aligned X/Z grid over lightweight tiles from saved terrain. Terrain detail changes with zoom and refreshes after saves or on demand; ungenerated areas remain blank. Drag, scroll, or use the map controls to move and zoom. Offline players appear at their last saved position with greyed-out heads, grouped by dimension.
- Remote maps now show locally cached terrain and the last map view while current tiles and player locations refresh. Recent logs and remote server icons also appear from the local cache before being updated. App settings shows the display cache size and lets you clear it.
- The server console suggests common commands and recently sent commands while typing. Use the arrow keys to choose a suggestion and Tab to complete it.
- The Server tab lets you upload an image for the server's Minecraft multiplayer icon; Scerite crops it to a 64×64 PNG.
- The Worlds tab shows saved seeds and gamerules, including the newer Minecraft gamerule formats. You can change gamerules on a running server, back up or download a stopped world as a ZIP, upload a replacement ZIP, and generate the active world again with new settings while preserving the previous world. Backups and preserved world folders can be restored individually or by choosing the latest backup for a world.
- Each server can automatically back up its active world when Scerite observes it stop. You can choose how many automatic backups to keep (default 3); older automatic ZIPs are removed after a new one is saved. Manual ZIPs remain until you delete them.
- Server settings now let you choose how much RAM Java can use when starting a server.

### Changed

- Remote environment cards and server navigation now show environment names without repeating SSH login details.
- Player statistics are now grouped into separate panels by Minecraft category, including Custom, Dropped, Killed, Picked up, and Used.
- Saved servers now show their Minecraft multiplayer icons in the Servers list when available.
- Console and Logs output use distinct colors for timestamps, warnings, errors, and chat, with a chat-only switch in both tabs.
- App settings now open from the bottom-left sidebar button, with shortcuts to saved servers in the sidebar. The Servers page uses the available width, and the theme control blends into its setting row.
- The Players tab now shows previously found players right away while it refreshes the full list and their access status.

### Fixed

- Discord chat messages are sent to the Minecraft console in one tmux operation. A brief failed console check is retried, and failed deliveries are logged with the Discord message ID.
- Offline player markers on the Map tab no longer show a person icon through the skin head.
- Long folder listings in the Files tab now scroll inside the browser, with the search and editor kept accessible.
- Long world settings and file lists can now scroll within the Worlds tab.

## [0.4.1] - 2026-10-20

### Fixed

- Starting a second server on the same machine now assigns it a free Minecraft port when its configured port is already in use.
- The Players tab explains when a player has no saved data yet and looks up heads by player name when available.
- The Players tab now shows saved players first, then checks the live online list with the server's `list` command and saves its response in Scerite's app data.

## [0.4.0] - 2026-10-19

### Added

- A Logs tab shows the latest server output and any older log files, including compressed archives.
- A Players tab lists known players, shows saved player details and statistics, and opens separate Whitelist, Operators, Banned names, and Banned IPs lists for adding or removing entries.
- Installed Modrinth mods can be updated or removed. Removal shows which mods depend on a selected mod and offers to remove dependencies no longer used by another mod.

### Changed

- The sidebar now shows the scerite logo instead of an initial.
- Search fields now filter files, properties, console and log lines, installed mods and plugins, players, and player access lists.
- The Install tab shows selectable software cards with Mod and Plugin badges and a searchable Minecraft version combobox.
- Mods and Plugins search now shows the server version and loader beside sorting, with fewer introductory labels.

### Fixed

- Canceling a mod or plugin removal now leaves it installed.
- The Minecraft version menu now has a solid background, so install controls do not show through it.
- Remote servers that have stopped no longer stay marked Running or show a stop error when their console session disappears.

## [0.3.2] - 2026-10-19

### Added

- The new Server tab shows the server's status, address and port, software and version, player count, and server-process CPU and RAM use.
- GitHub releases now include these changelog notes.

### Changed

- Renaming a server also renames its folder, both locally and over SSH.

### Fixed

- Installed mods and plugins smaller than 1 MB now show their size in KB.
- Mod and plugin searches now accept a single character.
- If a remote server exits during startup, its final console output stays visible so the error can be read.
- A disappearing remote console during startup or shutdown no longer appears as an SSH connection failure.

## [0.3.1] - 2026-10-19

### Added

- Update or remove Modrinth-managed plugins from the app.

### Fixed

- Find compatible plugins even when Modrinth does not specify their server environment.
- Keep SSH connections active during remote software installation.

### Changed

- Build desktop releases when a pull request is merged into `main`.

## [0.3.0] - 2026-10-19

- Initial desktop release.
