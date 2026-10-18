# AGENTS.md — scerite

Behavioral guidelines for AI coding agents working on scerite. These instructions prioritize correctness, simplicity, maintainability, and minimal changes.

**Tradeoff:** Favor caution over speed when changes involve security, architecture, or data integrity. For trivial tasks, use judgment and avoid unnecessary ceremony.

## 1. Think Before Coding

**Don't assume blindly. Don't hide uncertainty. Surface meaningful tradeoffs.**

Before implementing:
- Understand the request and inspect relevant existing code.
- State assumptions when they materially affect the implementation.
- If multiple reasonable interpretations exist, explain the important differences.
- If a simpler approach exists, prefer it and explain why when relevant.
- Push back when a proposed approach introduces unnecessary complexity or risk.
- If ambiguity materially affects correctness, security, architecture, or user-visible behavior, ask for clarification.
- Otherwise, make a reasonable assumption and proceed without unnecessary interruptions.

Avoid asking questions whose answers can be determined from the existing codebase.

## 2. Simplicity First

**Write the minimum code necessary to solve the problem correctly. Nothing speculative.**

- Do not implement features beyond what was requested.
- Avoid abstractions for single-use functionality.
- Do not introduce flexibility or configurability without a concrete requirement.
- Avoid unnecessary dependencies, wrappers, and architectural layers.
- Do not add defensive checks for genuinely impossible scenarios.
- Prefer existing project patterns over introducing new ones.
- Favor readability, maintainability, and testability over minimizing line count.
- Do not sacrifice correctness or security for brevity.

Ask yourself: "Would an experienced engineer consider this unnecessarily complicated?" If yes, simplify.

## 3. Surgical Changes

**Change only what is necessary. Clean up only what your changes affect.**

When modifying existing code:
- Do not refactor unrelated functionality.
- Do not reformat unrelated files or sections.
- Follow existing naming conventions, formatting, and architectural patterns.
- Preserve existing behavior unless the request explicitly changes it.
- If unrelated issues are discovered, mention them rather than fixing them without permission.
- Make supporting refactors when required for correctness, but keep them narrowly scoped.

When your changes create unused code:
- Remove imports, variables, functions, and files made unnecessary by your changes.
- Do not remove pre-existing unused code unless requested.

Every meaningful change should be traceable to the user's request or a necessary supporting modification.

## 4. Goal-Driven Execution

**Define success criteria. Implement, verify, and report results.**

For nontrivial tasks:
- Identify the expected behavior before implementation.
- Break multi-step work into a brief, verifiable plan.
- Prefer tests that reproduce bugs before fixing them when practical.
- Preserve existing behavior during refactoring.
- Verify the implementation against the original requirements.

Example:

1. Implement the requested behavior → Verify expected output.
2. Handle relevant failure cases → Verify error behavior.
3. Run appropriate checks → Confirm no regressions.

After implementation:
- Run relevant tests, type checks, and build checks when practical.
- Report which checks passed, failed, or could not be executed.
- Never claim a test or build succeeded without running it.
- Clearly distinguish verified behavior from assumptions.
- Summarize significant changes and remaining limitations.

For trivial changes, avoid unnecessary planning or lengthy reports.

---

## 5. Project Context

scerite is a cross-platform desktop application for managing local and remote Minecraft servers.

### Technology Stack

- **Desktop:** Tauri 2
- **Frontend:** React, TypeScript, Vite
- **Styling:** Tailwind CSS v4
- **UI Components:** shadcn/ui with Base UI
- **Backend:** Rust
- **Remote Management:** SSH and SFTP
- **Persistent Server Sessions:** tmux
- **Terminal Rendering:** xterm.js

Use existing project dependencies and conventions whenever possible.

Do not introduce alternative frameworks or major dependencies without a concrete technical reason.

### Architecture

- Keep privileged operations, SSH connections, filesystem access, and process management in Rust.
- Use Tauri commands for frontend-to-backend operations.
- Use Tauri events for streaming console output and asynchronous status updates.
- Keep React components focused on presentation and user interaction.
- Separate Minecraft-specific operations from transport and infrastructure concerns.
- Support local and remote server management through shared interfaces where they provide clear value.
- Remote Linux management must work without requiring a custom server-side agent.
- Minecraft server processes must remain independent of the desktop application's lifecycle.
- Use tmux for persistent interactive remote server sessions.
- Avoid introducing a separate backend service unless explicitly required.

### SSH and Remote Operations

- Reuse established SSH connections when appropriate.
- Handle disconnections and reconnections gracefully.
- Never assume that disconnecting SSH should stop a Minecraft server.
- Treat remote command execution and file operations as potentially fallible.
- Avoid unnecessary remote commands and repeated polling.
- Keep connection management independent of React component lifecycles.
- Prevent duplicate Minecraft server launches.

### Security

- Verify SSH host keys and detect unexpected changes.
- Never silently disable host-key verification.
- Never log passwords, private keys, access tokens, or other credentials.
- Store sensitive credentials using secure operating-system facilities.
- Treat remote paths, filenames, command arguments, and downloaded metadata as untrusted.
- Prevent shell injection when constructing remote commands.
- Validate filesystem operations and avoid unintended access outside managed server directories.
- Require confirmation before destructive operations such as deleting worlds or overwriting important files.
- Do not expose additional network services or management ports without an explicit requirement.

### Minecraft Server Management

- Preserve existing server worlds, configuration files, and installed mods unless changes are explicitly requested.
- Prefer graceful shutdown through Minecraft's `stop` command.
- Do not terminate Minecraft processes forcefully unless necessary.
- Verify version and loader compatibility before installing or updating server software.
- Avoid modifying server files while the server is running when doing so could cause corruption or inconsistent state.
- Design installation and update operations to report progress and meaningful errors.
- Do not assume all Minecraft loaders share identical installation or startup procedures.

---

## 6. Verification

Use the appropriate checks for the affected code.

### Frontend

- Run TypeScript checks when modifying TypeScript.
- Run relevant frontend tests when available.
- Verify that UI changes do not introduce obvious layout or interaction regressions.

### Rust Backend

- Run `cargo check` when modifying Rust code.
- Run relevant Rust tests when available.
- Check error handling for SSH, filesystem, and process-management operations.

### Cross-Layer Changes

- Verify that Tauri command names and payload types match between Rust and TypeScript.
- Check that asynchronous operations do not block the UI.
- Ensure that errors are propagated and displayed meaningfully.

If verification cannot be completed, explain what remains unverified and why.

---

## 7. Working Principles

- Prefer incremental changes over large rewrites.
- Do not add speculative features.
- Avoid premature optimization.
- Do not introduce dependencies for functionality already handled adequately by existing tools.
- Prefer explicit, understandable code over clever implementations.
- Keep error messages actionable.
- Preserve backward compatibility when practical.
- Keep documentation consistent with meaningful architectural or behavioral changes.

**These guidelines are working if:** changes remain focused, implementations stay understandable, unnecessary abstractions are avoided, security assumptions are explicit, and completed work is verified rather than merely claimed.

<!-- trail:start -->
## Trail — repo context graph

This repo is indexed in `.trail/cache/`: small linked markdown nodes that explain each
system and carry exact file:line spans, kept in sync with the code through git.

For ANY task here — understanding how something works, finding where code lives,
or scoping a change — get context from the graph before grepping or opening
source files. Re-ask freely (it's cheap) and reuse literal identifiers you
already have (symbol, error string, file name) as the query. New to this repo?
Run `trail map` first — a token-budgeted orientation (dir clusters, hubs,
hotspots), no LLM, no key.

- Run `trail ask "<your question>" --source` → ranked nodes with the relevant
  code spans inlined (each hit's ≤8-line crux by default; `--full` for whole
  definitions when the crux isn't enough). Match the tool to the task shape:
  for understanding or editing, the top node IS the answer — cite its
  `covers:` file:line spans and edit straight from `--source`. For
  exhaustive tasks ("every occurrence / every caller of this pattern"), ranked
  results are top-N, not complete — run `trail grep "<literal>"` instead
  (exhaustive over indexed files, grouped by enclosing symbol), falling back
  to raw `grep -rn` only for unindexed files.
- `trail skeleton <file>` → every definition's signature + span, ~10× cheaper
  than reading the file; use it to skim an API surface.
- `trail callers <symbol>` gives precomputed, exact edges — who calls this.
  Add `--direction out` for what it calls, or `--depth N` to walk
  transitively for the full blast radius. For structural questions, skip
  ranking and use this directly.
- Or browse: `.trail/cache/INDEX.md` lists every node; follow the links.
- Monorepos and folders of multiple repos rank fairly across sub-projects —
  hits carry `[scope/]` labels naming which one they're from. Narrow with
  `trail ask "<task>" --in <scope>/` once you know where you're working.

If a returned span is truncated ("+N more lines"), open the file at that exact
range before finalizing. Only open source files when a node genuinely lacks a
needed detail, and then at the exact file:line the node points to — never
re-read whole files.

After big code changes, refresh the graph with `trail build` (deterministic,
no API key, $0).
<!-- trail:end -->
