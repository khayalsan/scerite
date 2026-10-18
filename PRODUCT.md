# Product

<!-- impeccable:product-schema 1 -->

## Users

Server owners and administrators managing their Minecraft servers.

## Product Purpose

scerite is a cross-platform desktop application for managing local and remote Minecraft servers. Success means users can manage server files and server processes from one app while their servers and worlds remain safe when the desktop app closes or a connection drops.

## Positioning

scerite manages remote Linux servers through standard SSH, SFTP, and tmux without requiring a custom server-side agent.

## Operating Context

Users manage server processes and files on their own computer or on remote Linux machines. Remote access uses SSH. Minecraft servers should keep running independently of the desktop application's lifecycle.

## Capabilities and Constraints

- Support local and remote server management through shared interfaces where useful.
- Verify SSH host keys and detect unexpected changes.
- Keep credentials in secure operating-system storage; never log secrets.
- Treat remote paths, file names, command arguments, and downloaded metadata as untrusted.
- Prefer graceful Minecraft shutdown and protect worlds, configuration, and installed mods.
- Verify Minecraft version and loader compatibility; installation and startup behavior can differ by loader.
- Require confirmation before destructive operations such as deleting worlds or overwriting important files.

## Brand Commitments

The product name is scerite. No other brand assets or visual commitments are established yet.

## Product Principles

- Keep Minecraft processes independent of the desktop app and its SSH connections.
- Keep privileged operations, transport, and filesystem access in Rust.
- Protect server worlds, configuration, installed mods, and credentials.
- Reuse workflows across local and remote servers while preserving their platform differences.
