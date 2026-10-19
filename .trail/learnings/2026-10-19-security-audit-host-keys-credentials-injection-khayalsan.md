---
title: "Security audit: host keys, credentials, injection, git history"
author: Khayalsan
date: 2026-10-19
branch: feat/release-changelog
cost: { minutes: 1, tokens: 45824 }
---

## Decided
Audit (read-only, no code changed) found: host keys verified against ~/.ssh/known_hosts in SshHandler::check_server_key (remote_ssh.rs), fail closed, no TOFU, certificates refused. No passwords/passphrases stored; only identity_file paths plus SSH agent, so no OS keychain is used. Remote commands use shell_quote with -- guards; paths validated by resolve_remote_relative/canonical_inside. Git history had no tracked secrets or private keys.

## Tried and ruled out
Did not run cargo audit or a secret scanner; command-injection review was sampled, not exhaustive.

## Watch out
tmux commands interpolate session name unquoted (remote_ssh.rs ~286-445); safe only because session_name() is scerite-<uuid hex>. Wrap in shell_quote if that changes. Adding password/passphrase support would require OS keychain storage. Encrypted key files are not decrypted in-app; they must be in the agent.
