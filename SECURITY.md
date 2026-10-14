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