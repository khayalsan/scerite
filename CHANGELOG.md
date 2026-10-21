# Changelog

Add release notes under **Unreleased** before merging a pull request into `main`. The release workflow moves them into the new version and publishes them with the GitHub release.

## [Unreleased]

### Added

- The Worlds tab shows saved seeds and gamerules, including the newer Minecraft gamerule formats. You can change gamerules on a running server, back up or download a stopped world as a ZIP, upload a replacement ZIP, and generate the active world again with new settings while preserving the previous world. Backups and preserved world folders can be restored individually or by choosing the latest backup for a world.
- Each server can automatically back up its active world when Scerite observes it stop. You can choose how many automatic backups to keep (default 3); older automatic ZIPs are removed after a new one is saved. Manual ZIPs remain until you delete them.
- Server settings now let you choose how much RAM Java can use when starting a server.
- The Server tab shows recent TPS for Paper-compatible servers when their console reports it.

### Changed

- The Players tab now shows previously found players right away while it refreshes the full list and their access status.

### Fixed

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
