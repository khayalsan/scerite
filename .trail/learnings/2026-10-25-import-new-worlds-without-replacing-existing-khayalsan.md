---
title: Import new worlds without replacing existing folders
author: Khayalsan
date: 2026-10-25
branch: feat/world-upload-cpu-velocity
---

## Decided
- Keep the existing Upload ZIP action for replacing a selected world. The new Import action adds a named world without replacing any existing folder.
- Reuse the validated ZIP extraction path for local and remote imports. Pass a require-absent condition to reject duplicate names before staging and moving files.
## Tried and ruled out
- A frontend-only upload control cannot safely distinguish creation from replacement; the Rust command must enforce absence.
- Treating Java's ActiveProcessorCount as extra CPU allocation would mislead users because the JVM already uses host cores by default.
## Watch out
- The world archive helper also serves backup restoration, which may replace an existing world. Preserve that behavior when changing the import path.
- Windows tests that move folders should use the repository target directory for temporary fixtures; the OS temp path failed under this environment's filesystem permissions.
