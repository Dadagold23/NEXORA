# Changelog

All notable changes to NEXORA/NXL are recorded here. The project follows
semantic versioning; versions below are the language/toolchain releases that
have actually been built and tested.

## [0.1.0] - 2026-10-14

### Language (phases 1-6, all approved)

- **Phase 1 - errors and strings**: string interpolation and concatenation,
  `throw`, `try`/`catch`/`finally`, catchable runtime errors.
- **Phase 2 - iteration and data**: range and array iteration, arrays and
  indexing, structs, enums, first-match-wins `match`, gradual operators.
- **Phase 3 - objects and methods**: classes with constructors, dynamic method
  dispatch with receiver write-back, `impl` blocks, interface/trait
  validation.
- **Phase 4 - modules**: project loader, `import`/`from`/aliases, cross-module
  calls, types and methods, cycle detection, project-aware CLI.
- **Phase 5 - concurrency**: `spawn`/`await` on VM worker threads with
  isolated state, channels, `async { }` blocks with by-value capture, actor
  `mailbox` fields, structured task joining.
- **Phase 6 - backend control flow**: native `while` and `for ... in`
  emission for the JavaScript backend with VM parity, loop metadata recorded
  during lowering, Rust `throw`/`try`/`catch`/`finally` via `catch_unwind`,
  and honest `NXL2007` errors where a backend cannot emit correct code.

### Toolchain

- `nxlc` commands: `new`, `init`, `run`, `build`, `check`, `fmt`, `lint`,
  `clean`, `test`, `debug`, `docs`, `doctor`, `platforms`, `pkg *`, `db *`,
  `serve`, `lsp`.
- **Project templates (R08)**: `new console <name>` creates a runnable project
  (`nxl.toml`, `src/main.nxl`, `tests/basic.nxl`, `README.md`, `.gitignore`)
  with the project name substituted. Templates also work as
  `new --template console <name>` and `new --template=console <name>`, and the
  template is embedded in the binary so an installed compiler needs no
  repository checkout.
- **`nexora` command surface (R07)**: `nexora new console hello`, `nexora run`,
  `nexora check`, `nexora build --release`, `nexora test`, `nexora doctor`.
  `nexora` and `nxlc` are the same binary; the project-facing spelling was added
  so the documented workflow works as written.
- `nexora dev` and `nexora new web` are **refused with explanations** rather
  than pretending: NXL has no web development server, so a generated web
  project could not run.
- `nxlc doctor` now probes the real environment: it reports its own version
  and architecture, invokes Node.js to decide whether the JavaScript backend
  is usable, and inspects the working directory for `nxl.toml` and the
  declared entry point. Previously it reported hardcoded successes.
- **Package manager fixes**: `pkg add` no longer reports success for packages
  that cannot be resolved (it exits non-zero and explains); local path
  dependencies (`--path`) work end to end including transitive path
  dependencies; `pkg tree`, `pkg audit`, `pkg publish`, `pkg search` and
  `pkg info` are all reachable (`tree`/`audit`/`publish` were previously
  unreachable behind a parser gap); `pkg publish` fails instead of printing
  "not implemented" with exit 0; lockfile checksums are now SHA-256 instead of
  a non-cryptographic FNV-1a stand-in.
- **Release pipeline**: `scripts/package-release.ps1` (Windows) and
  `scripts/package-release.sh` (Linux/macOS) run quality gates, stage the
  platform layout, generate SHA-256 checksums, verify the artifact that will
  ship, and produce a per-platform archive. The Unix packaging script refuses
  to run on a host that does not match the requested target.
- **Installers**: `install/windows-x64/` (`install.ps1` + `install.bat`) and
  `install/unix/install.sh` both verify SHA-256 before installing, install for
  the current user only, never modify system settings, support `--dry-run`,
  `-Uninstall`/`--uninstall`, and prove success by running the installed
  binary.
- **UTF-8 BOM tolerance**: a BOM-prefixed `.nxl` file used to fail with
  `unexpected character`. Windows editors and PowerShell's default `utf8`
  encoding write BOMs, so this broke ordinary files.
- Windows x64 release package: `dist/windows-x64/` with `nxlc.exe`,
  `install.ps1`, `install.bat`, checksums, and a runnable example.
- `LICENSE` (MIT), `SECURITY.md`, `CHANGELOG.md`, `docs/platform-support.md`.

### Known limitations

- Concurrency runs on the VM only; the JavaScript backend refuses it.
- The Rust backend supports straight-line code and `try`/`catch`/`finally`
  only; everything else is refused rather than mis-emitted.
- No live package registry and no package signing.
- No web development server and no web project template; both are refused with
  an explanation.
- `nxlc test` compiles and runs each file in `tests/`; there is no test-block
  syntax.
- macOS x64 and Linux ARM64 are built in CI; no clean-machine install has been
  recorded yet, and macOS artifacts are not code-signed or notarised.
- Windows ARM64 is not built.

[0.1.0]: https://github.com/Dadagold23/NEXORA/releases/tag/v0.1.0