# ServerCrafter

ServerCrafter is a cross-platform desktop app for managing local and remote Minecraft servers. The desktop app is built with Tauri 2, React, TypeScript, and Rust.

## Architecture

- `src/` contains the React interface. `src/lib/serverProfiles.ts` is the frontend API boundary for server profile commands.
- `src-tauri/src/lib.rs` initializes Tauri state and registers commands.
- `src-tauri/src/server_profiles/model.rs` defines the shared Rust profile and target types.
- `src-tauri/src/server_profiles/commands.rs` exposes profile actions to the frontend.
- `src-tauri/src/server_profiles/store.rs` validates and persists profiles in the app data directory. It writes a temporary file and keeps a recoverable backup while replacing saved data.
- Minecraft process management, local filesystem access, and remote SSH/SFTP/tmux operations belong in Rust adapters and services as those features are added.

Profile data contains server names, local or remote server folders, launch commands, and remote SSH connection details. It does not contain passwords or private keys. Remote authentication uses the SSH agent or an optional private-key file path; encrypted keys must be unlocked through the agent. Host keys are checked against the user's `known_hosts` file.

## Current scope

The app can create, edit, list, and remove server profiles. Local servers can be launched in sessions that survive the desktop app, with a live console and a guarded file editor. Remote Linux profiles can use SSH to manage persistent `tmux` sessions, send console commands, and browse or edit files over SFTP.

Remote process control requires `tmux` on the server and a trusted host key in `known_hosts`. Remote file operations stay inside the configured server folder, skip symbolic links, limit text files to 1 MB, and require the managed server to be stopped before writing.

## Development

- `npm run tauri dev` starts the desktop app.
- `npm run build` type-checks TypeScript and builds the frontend.
- `cargo check --manifest-path src-tauri/Cargo.toml` checks the Rust backend.

## Disclaimer

NOT AN OFFICIAL MINECRAFT PRODUCT. NOT APPROVED BY OR ASSOCIATED WITH MOJANG OR MICROSOFT.
