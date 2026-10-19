# Changelog

Add release notes under **Unreleased** before merging a pull request into `main`. The release workflow moves these notes into the new version and uses them for the GitHub release.

## [Unreleased]

### Added

- A Server tab shows connection details, player count, and CPU and RAM use.
- GitHub releases now show these notes automatically.

### Changed

- Renaming a server also renames its folder, whether it is on this computer or reached through SSH.

### Fixed

- Show installed mods and plugins in KB when they are smaller than 1 MB.
- Allow one-character searches for mods and plugins.

## [0.3.1] - 2026-10-19

### Added

- Update and remove Modrinth-managed plugins from the app.
- Keep SSH connections alive during remote software installation.

### Fixed

- Show compatible plugins even when Modrinth does not specify their server environment.

### Changed

- Build desktop releases when a pull request is merged into `main`.

## [0.3.0] - 2026-10-19

- Baseline release before the plugin management changes.
