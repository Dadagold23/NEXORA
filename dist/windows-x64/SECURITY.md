# Security Policy

## Reporting a vulnerability

Report suspected vulnerabilities privately to the maintainers rather than by
opening a public issue. Include the affected version, platform, reproduction
steps, and any proof-of-concept code. We aim to acknowledge reports within a
few days.

Do not publish a working exploit for an unfixed issue before a fix or
mitigation is released.

## Release trust model

- Release archives are published with a `SHA-256` checksum file beside them.
  Verify before installing:

  ```powershell
  Get-FileHash .\NEXORA-<version>-windows-x64.zip -Algorithm SHA256
  ```

- The Windows installer refuses to install anything unless `nxlc.exe` matches
  its `checksums.sha256` entry. A mismatch aborts before any file is written.
- Artifacts are **not** code-signed and no signed release metadata is
  published. Treat the checksum file as the integrity control, and verify it
  over a channel you trust. Signing is planned but not implemented; no
  signature claims are made.
- No credentials, API keys or signing material are included in release
  assets or committed to this repository.

## Installer safety properties

- Installs to a user-writable directory (`%LOCALAPPDATA%\NEXORA\bin`) by
  default; no administrator privileges are requested or elevated.
- Only the **user** PATH is modified, and only by appending one directory.
  System-wide settings are never changed.
- Refuses to overwrite files in the install directory it did not create.
- Never deletes or modifies user projects; `-Uninstall` removes only the
  installed executable and this installer's own PATH entry.
- `-DryRun` performs every check and prints the planned actions without
  writing files or changing settings.

## Compiler-level safety notes

- The VM executes programs directly; a malicious `.nxl` file runs with the
  privileges of the user who runs it. Only run code you trust.
- `nxl pkg add` records a dependency in `nxl.toml` but has no live registry
  yet, so it does not download or execute third-party code. Do not treat it as
  a verified package source.
- Dynamic dispatch, channels and threads follow value semantics with isolated
  per-thread state; there is no shared mutable state between tasks.

## Known unresolved issues

- Packages are neither signed nor verified against an advisory database;
  `nxl audit` currently inspects a local advisory set only.
- The JS backend cannot execute concurrency; programs that use
  `spawn`/`await`/`channel` must run on the VM.

## Security review of the current release (2026-10-15)

Scope: the Windows and Unix installers, the release packaging scripts, the
package manager's local-dependency handling, and the project templates.

| Area | Finding | Status |
| --- | --- | --- |
| Installer integrity | Both installers verify SHA-256 against `checksums.sha256` before writing anything, and refuse to install without that file | Verified by tests (tamper and missing-checksum cases) |
| Installer privileges | User-scoped install only; no elevation, no system PATH or system directories | Verified by review of both scripts |
| Installer blast radius | Refuses to overwrite files it did not create; `--uninstall` removes only its own executable and PATH entry | Verified by review |
| Unverified remote execution | No installer downloads and executes a remote binary. Archives are downloaded once, verified, then run locally | Verified |
| Command injection | No user input is interpolated into shell commands; the Rust code spawns processes with argument arrays | Verified by review |
| Archive extraction | Extraction uses `tar -xzf` / `Expand-Archive` on archives this project produced; no third-party archive is ever extracted | Accepted risk, documented |
| Path dependencies | A manifest may point at any local directory containing an `nxl.toml`; the package source is copied into `.nxl-cache` and is **not** executed at install time | Accepted: requires control of the project's own manifest |
| Lockfile integrity | Checksums are SHA-256 (previously a non-cryptographic FNV-1a stand-in that could not detect tampering) | Fixed |
| Package supply chain | No live registry, so no third-party code can be fetched or installed. `pkg add` for a registry package fails rather than pretending | By design |
| Credentials | None in the repository or release assets; scanned before each release | Verified |
| Script hygiene | Release scripts have no BOM and use LF endings, so the shebang and `bash -n` work | Fixed this cycle |
| Code signing | Release binaries and archives are **not** signed, and release metadata carries no signature | Not implemented; stated plainly |

### Accepted risks worth knowing

- The VM executes whatever `.nxl` file it is pointed at, with the privileges of
  the invoking user. Only run code you trust.
- `checksums.sha256` proves the download matches what the packaging pipeline
  produced. It does not prove the pipeline itself was uncompromised. Signing
  releases would close that gap; it is not implemented.
- macOS artifacts are unnotarised, so Gatekeeper may refuse them on first run.
- The package manager records a dependency written in `nxl.toml` and then
  fails when it cannot resolve it. Because no live registry exists, no remote
  code path exists today; a future registry must add verification before that
  changes.