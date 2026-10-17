# ServerCrafter

ServerCrafter is a cross-platform desktop app for managing local and remote Minecraft servers. The desktop app is built with Tauri 2, React, TypeScript, and Rust.

## Architecture

- `src/` contains the React interface. `src/lib/serverProfiles.ts` is the frontend API boundary for server profile commands.
- `src-tauri/src/lib.rs` initializes Tauri state and registers commands.
- `src-tauri/src/server_profiles/model.rs` defines the shared Rust profile and target types.
- `src-tauri/src/server_profiles/commands.rs` exposes profile actions to the frontend.
- `src-tauri/src/server_profiles/store.rs` validates and persists profiles in the app data directory. It writes a temporary file and keeps a recoverable backup while replacing saved data.
- Minecraft process management, local filesystem access, and remote SSH/SFTP/tmux operations belong in Rust adapters and services as those features are added.

Profile data contains server names and local folder or remote SSH connection details. It does not contain passwords or private keys. Credential storage will use operating-system secure storage when authentication is implemented.

## Current scope

The app can create, edit, list, and remove local and remote server profiles. Server process control, SSH connections, and server file operations are not implemented yet.

## Development

- `npm run tauri dev` starts the desktop app.
- `npm run build` type-checks TypeScript and builds the frontend.
- `cargo check --manifest-path src-tauri/Cargo.toml` checks the Rust backend.
