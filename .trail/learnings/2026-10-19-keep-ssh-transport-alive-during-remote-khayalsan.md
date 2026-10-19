---
title: Keep SSH transport alive during remote installs
author: Khayalsan
date: 2026-10-19
branch: fix/ssh-channel-timeout
touches:
  - src-tauri/src/remote_ssh.rs
---

## Decided
The russh client transport must not use the 30-second cached-connection idle timeout as its own inactivity deadline. Paper and Purpur remote installs can run quiet download or checksum commands longer than 30 seconds, then need another SSH channel. Leave the transport inactivity timeout unset and send periodic keepalives; keep the manager's 30-second idle eviction for unused cached connections.

## Tried and ruled out
A one-shot keepalive immediately after authentication does not protect long quiet operations. Retrying an entire installation automatically is unsafe because remote commands may already have created files or moved a runtime.

## Watch out
The error comes from channel_open_session after the connection has closed. Cargo check confirms the configuration compiles, but a real Linux SSH host and a slow Paper or Purpur install are needed to confirm the reported failure is fixed. Keep host-key verification and the existing install file checks unchanged.
