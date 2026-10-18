# ServerCrafter

ServerCrafter is a cross-platform desktop app for managing local and remote Minecraft servers. The desktop app is built with Tauri 2, React, TypeScript, and Rust.

## Architecture

- `src/` contains the React interface. `src/lib/serverProfiles.ts` is the frontend API boundary for server profile commands.
- `src-tauri/src/lib.rs` initializes Tauri state and registers commands.
- `src-tauri/src/server_profiles/model.rs` defines the shared Rust profile and target types.
- `src-tauri/src/server_profiles/commands.rs` exposes profile actions to the frontend.
- `src-tauri/src/server_profiles/store.rs` validates and persists profiles in the app data directory. It writes a temporary file and keeps a recoverable backup while replacing saved data.
- Minecraft process management, local filesystem access, remote SSH/SFTP/tmux operations, and vanilla installation are handled in Rust.

Profile data contains server names, local or remote server folders, launch commands, and remote SSH connection details. It does not contain passwords or private keys. Remote authentication uses the SSH agent or an optional private-key file path; encrypted keys must be unlocked through the agent. Host keys are checked against the user's `known_hosts` file.

## Current scope

The app can create, edit, list, and remove server profiles. Local servers can be launched in sessions that survive the desktop app, with a live console and a guarded file editor. Remote Linux profiles can use SSH to manage persistent `tmux` sessions, send console commands, and browse or edit files over SFTP.

Remote process control requires `tmux` on the server and a trusted host key in `known_hosts`. Remote file operations stay inside the configured server folder, skip symbolic links, limit text files to 1 MB, and require the managed server to be stopped before writing.

## Vanilla installation

Set one Minecraft root for this computer, or a root for each remote environment. Each selected server folder must be directly inside that root's `servers/` folder. The installer creates `servers/` and the selected server folder when needed. From the server's **Install** tab, choose an official vanilla release. The jar is verified against Mojang's SHA-1 checksum, saved under `cache/minecraft/<version>/server.jar`, then copied into the selected folder as `server.jar`. The installer refuses to replace an existing `server.jar` and requires the server to be stopped. Remote installation uploads through SFTP and uses standard Linux `sha1sum`, `cp`, and `ln` commands; no remote agent is required. Loader and mod installation are not included yet.

The installer reads the required Java major version from Mojang's release metadata and installs a matching Eclipse Temurin JDK under `runtimes/java/<major>/<platform>/<build>/`. JDK archives are SHA-256 verified, used temporarily during local or remote installation, then removed. The next Java installation or selection removes the old `cache/java/` folder for that local root or remote environment. Java launch profiles are pointed at the managed runtime and use `server.jar`; custom launch executables are preserved. Remote Java installation requires Linux `sha256sum`, `tar`, `mv`, and `rm`.

## Development

- `npm run tauri dev` starts the desktop app.
- `npm run build` type-checks TypeScript and builds the frontend.
- `cargo check --manifest-path src-tauri/Cargo.toml` checks the Rust backend.
