# Changelog

All notable changes to NEXORA/NXL are recorded here. The project follows
semantic versioning; versions below are the language/toolchain releases that
have actually been built and tested.

## [Unreleased]

## [0.1.1] - 2026-10-15

Everything in this section is what changed since the `v0.1.0` tag. The release
run for `v0.1.0` failed on every platform - it used retired runner labels and
the installer bug described below - so `v0.1.0` was never published and this
release replaces it.

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

### Editor

- **The VS Code extension has a language server.** `nxlc lsp` was a stub that
  returned without reading anything, and the NLS bridge read the source inline
  in every request — something no real client sends — while advertising hover
  and definition that it did not answer. The server now keeps the documents the
  client opens, publishes diagnostics from the real compiler when a document
  opens, changes, saves and closes, and answers completion, hover,
  go-to-definition, document symbols and find-references. `scripts/lsp-smoke.js`
  drives it over real stdio and runs in the suite.
- **The extension speaks to it** through `language-client.js`, a
  dependency-free JSON-RPC client, so the extension still loads from source
  with no `npm install`. `scripts/lsp-client-check.js` exercises that client
  against the real binary, so one that cannot talk to the server fails rather
  than silently offering no completion.

### The web page is written in NXL

- **`public/index.nxl`** replaces `public/index.html` in the `web` target. The
  dev server compiles and runs it on the server and serves what it prints as
  HTML, so the page imports the same modules as the app instead of being markup
  somebody has to keep in step by hand. A hand-written `public/index.html`
  still works and is used when there is no NXL page; a page that fails to
  compile reports why rather than falling back.
- **Any file in a project resolves `app.*` against `src/`.** `nxlc run
  public/index.nxl` and `nxlc test` previously looked for `public/app/…` and
  reported every import as undeclared.
- **Multi-line strings could not be compiled at all.** The backends escaped
  backslashes and quotes but not newlines or control characters, so a raw
  string literal produced a JavaScript or Rust string literal containing a
  bare newline, which does not parse. Escaping now covers `\n`, `\r`, `\t` and
  the remaining control characters.

### Fixes

- **Commands run outside a project blamed the toolchain.**
  `ProjectConfig::load` returns defaults when there is no manifest, so
  `nxlc run`, `check`, `fmt` and `build` all fell through to `cannot read
  .../src/main.nxl` — a path that reads like a missing file rather than a
  missing project. They now say the directory is not an NXL project and name
  the command that creates one. Naming a file still works without a manifest,
  because a loose file is a deliberate single-file workflow.
- **`nxlc serve` told you to run the command you had just run.** Its message
  ended with "run `nxl serve` to listen" after you had run `serve`. It now
  says plainly that NUF is a specification and nothing is listening, and points
  at `nexora dev`, which is a real server.
- **The release package shipped `templates/`.** A `templates/web/src`
  directory next to a real project looks like a project: `cd` into it and
  `nxlc run` reports a missing entry point instead of the fact that it is a
  template. Both targets are compiled into the compiler, so `new` works from an
  installed binary with nothing else on disk, and the package no longer carries
  them.
- **Compiler source was served as a web asset.** `public/index.nxl` is a
  program the dev server renders for `/`, but requesting it by path returned
  its text, so a browser showed the page's source instead of the page and it
  looked like a broken site rather than a wrong URL. `.nxl` files under
  `public/` are now refused; the page is served by rendering it.
- **`nxlc dev` outside a project** said `cannot read .../src/main.nxl`, which
  reads like a broken toolchain. It now says the directory is not an NXL
  project and names the command that creates one, and the server prints the
  project it is serving on startup, so it is obvious which one answered.

### Toolchain

- **`nexora new web <name>`**: a second project target that scaffolds a
  complete, runnable web application rather than a hello-world. The worktree
  holds a manifest, four NXL modules (`src/main.nxl` plus `src/app/config`,
  `routes` and `state`), a landing page with a splash screen
  (`public/index.html`, `styles.css`, `bootstrap.js`, `favicon.svg`), a
  self-check and a README. `new` also resolves and locks the new manifest's
  dependencies before it returns, so the project is ready to run. `--target`
  and `--template` are now the same flag; both spellings work.
- **The dev server serves the project's own page.** `nxlc dev` serves
  `public/index.html` at `/` and every file under `public/` at its own path,
  falling back to its generated host page for projects that have none. Paths
  are canonicalised before they are read, so `..` cannot climb out of `public/`
  and an escaping symlink is refused. A project page that forgets to load
  `/app.js` has it injected rather than silently rendering empty.
- **The scaffolded page is wired to the program, not just decorated.** NXL has
  no DOM types, so `print()` compiles to `console.log` and `public/bootstrap.js`
  takes it over before the program runs: the boot log becomes the hero, the
  feature list, the route table and the status pill. The splash screen clears
  when the program prints `READY` — not on a timer — so a program that throws
  leaves it up with the reason rather than looking healthy.
  `scripts/verify-web-page.js <project>` checks this without a browser, and
  the test suite runs it against the scaffolded template.

### Compiler fixes found while building the web target

- **Array type annotations could not compile at all.** A return type of `[T]`
  or `Array<T>` was rejected with `NXL3001 ... returns Array(0), expected
  Array(<id>)`, as was any call passing an array literal to a `[T]` parameter,
  and any uncalled function with such a return type. An array literal is
  inferred as `Array(0)` because its element type is not pinned down, while the
  declaration is `Array(<id>)`; the contracts compared the two with `!=`
  instead of the compatibility rule the rest of the compiler already uses.
- **`+` refused dynamically typed operands.** `"prefix" + value` was the only
  operator that rejected a value of unknown type — comparisons, calls and
  interpolation all accept them. It now does too, and the result stays
  `Unknown`. `Int + String` and `Array + Int` are still rejected.
- **`!` on an unknown value, and the unary contract that checked it, both
  required a known `Bool`.** Both now accept `Unknown`, like the comparison
  operators do.
- **`nxl test` could not see a project's modules.** It compiled each file in
  `tests/` on its own, so a test importing `app.config` failed with "use of
  undeclared name". It now loads the project, like `nxl check` does, and
  resolves imports against the entry's directory.

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

[Unreleased]: https://github.com/Dadagold23/NEXORA/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/Dadagold23/NEXORA/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/Dadagold23/NEXORA/releases/tag/v0.1.0