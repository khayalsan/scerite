---
title: Windows SSH crypto backend compatibility
author: Khayalsan
date: 2026-10-17
branch: feat/local-server-operations
touches:
  - src-tauri/Cargo.lock
  - src-tauri/Cargo.toml
---

## Decided
On Windows, libssh2-sys defaults to WinCNG, whose backend disables Ed25519 and Curve25519 in its bundled configuration. A server whose OpenSSH negotiation selects `curve25519-sha256` and `ssh-ed25519` can therefore fail during KEX before known_hosts or user authentication. Build ssh2 with `openssl-on-win32` and `vendored-openssl` on Windows; keep normal ssh2 defaults on non-Windows platforms.

## Tried and ruled out
Changing only host-key preference does not add missing Curve25519 KEX or Ed25519 verification to WinCNG. Do not weaken server algorithms or host-key checks to accommodate the unsupported backend.

## Watch out
The vendored OpenSSL build on Windows requires Perl and the Visual C++ build tools. If cargo check fails during the OpenSSL Configure step, inspect the local build prerequisites; it is not a Rust type error. Verify the resulting Windows build by connecting to a server that negotiates both Curve25519 and Ed25519.
