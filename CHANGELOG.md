# Changelog

All notable changes to NEXORA/NXL are recorded here. The project follows
semantic versioning; versions below are the language/toolchain releases that
have actually been built and tested.

## [Unreleased]

Everything in this section is on `main` but is **not** in the `v0.1.0` tag,
because it landed after that tag was pushed.

### Fixes

- **`install.sh --prefix` never worked.** The argument loop was written as
  `for arg in "$@"`, which expands the argument list once, so the `shift` could
  not consume the value after `--prefix`: the path was then read as another
  option and the installer aborted with `unknown option`. Since
  `scripts/verify-install.sh` passes `--prefix` in four places, the Unix
  onboarding check could not have passed on Linux or macOS. It now shifts a
  positional list instead.
- **Bash scripts were being run under `sh`.** On Ubuntu `/bin/sh` is dash,
  which has no `set -o pipefail`, so `package-release.sh`, `verify-install.sh`
  and the staged `install.sh` died on their own first line. The release
  workflow and the internal call sites now use `bash`.
- **The Unix installer test could only pass on Windows.**
  `unix_installer_refuses_a_wrong_platform` asserted that a dry run *succeeds*
  on Linux and macOS, but it runs the copy in `install/unix/`, which ships no
  `nxlc` and no `checksums.sha256`, so the installer always refuses it. On
  Windows the assertion is the opposite one, which is why CI was red on Ubuntu
  and green on Windows. The test now checks what its name says, and two new
  hermetic tests stage a real package to prove the dry run verifies the
  checksum and writes nothing, and that a tampered payload is refused.
- **Release runner labels followed retired images.** `macos-13` was retired in
  December 2025, `ubuntu-22.04` is inside its deprecation window, and
  `macos-14` retires on 2 November 2026; those jobs could no longer start.
  They now use `ubuntu-24.04`, `ubuntu-24.04-arm`, `macos-15` and
  `macos-15-intel`.

### Toolchain

- **`nexora repl`**: a persistent session — definitions, imports, the module
  scope and local bindings survive between lines, and `--replay <file>` feeds a
  transcript through the same session path.
- **`nexora dev`**: a real development server (Axum/Tokio). It serves the
  project over HTTP, watches the source tree, and pushes a reload over SSE when
  a file changes (matched by content hash, so a touch alone does not trigger
  one). It compiles to JavaScript only, and it does not yet execute NXL on the
  server side.
- **`pkg publish`**: no longer a stub. Packages are written to a registry
  directory with an `index.json`, per-package `package.json` and SHA-256
  checksums, and `pkg install`/`add`/`search`/`info` read them back through
  `--registry`. Still no hosted registry and no signing.

### Editor and brand

- **VS Code extension** (`editors/vscode-nxl`): `.nxl` language registration,
  a syntax grammar generated from the compiler's own lexer, the `.nxl` file
  icon, and Run/Check/Build commands that invoke the real compiler. No language
  server yet, so completion, hover and inline diagnostics are absent.
- **Brand assets**: `scripts/make-brand-assets.ps1` regenerates every image
  from `favicon.ico` — the extension icons (`nxl-32/64/128.png`, `nxl.svg`), the
  marketplace banner (1280x600), the GitHub social preview (1280x640) and the
  512x512 logo used by the READMEs. VS Code rejects `.ico` language icons, and
  the checked-in ICO has a malformed header and an opaque white background, so
  the generator flood fills the border-connected background into alpha before
  scaling; otherwise every icon keeps white corners. Verified by
  `node editors/vscode-nxl/scripts/verify-extension.js`, which now also checks
  the generated image dimensions.

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