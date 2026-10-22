# CLI Reference

The compiler binary is `nxlc`. It also answers to `nexora`: the program reads
its own invocation name, so a copy or symlink called `nexora` runs the same
commands. The release installers create only `nxlc`.

## Project commands

| Command | What it does |
| --- | --- |
| `new [<target>] <name>` | Create a project from a target. `new hello`, `new console hello`, `new --template console hello`, `new --template=console hello`, `new --target console hello` and `new --target=console hello` are equivalent. Targets: `console`, `web`, `mobile`. Dependencies declared by the new manifest are resolved and locked before the command returns. |
| `init` | Create the same project layout in the current directory. |
| `run [file]` | Run a project entry point or a single `.nxl` file on the VM. The VM is authoritative: a runtime failure (for example, division by zero) is reported with a non-zero exit. The JavaScript runtime is consulted only when the VM cannot compile the module at all, so a program's own error is never masked by re-running it elsewhere. |
| `build [--target android] [--release]` | Compile the project into `build/`. In a mobile project this builds an Android APK instead; see [Mobile](25-mobile.md). Any target other than `android` is refused (`NXL12025`). |
| `check [file]` | Parse and type-check without producing output. |
| `test [filter]` | Parse, type-check and run every `.nxl` file in `tests/`. In a mobile project it also compiles every screen asset with the generated prelude. There is no test-block syntax yet. |
| `clean` | Remove build output. |
| `fmt [file]`, `lint [file]` | Normalise whitespace; lint delegates to check. |
| `dev [--port N] [--host H:P] [--no-watch]` | Start the development server: compiles the project to JavaScript, serves it with live reload over SSE, and shows a compile-error page that recovers on the next fix. Binds `127.0.0.1` by default. In a mobile project `dev` instead builds the debug APK, installs it on the connected Android device and launches it. |
| `doctor` | Real environment diagnostics: compiler version and architecture, Node.js availability, `nxl.toml` and its declared entry point. |
| `debug <file> [--break=N] [--inspect] [--steps=N]` | Statement trace with breakpoint counts; `--inspect` runs a stepping session with a call stack + variables view, `--steps=N` steps N statements from the initial pause. |
| `docs` | Print the documentation map. |
| `platforms` | Print the platform support summary. |
| `version`, `--version`, `help` | Toolchain metadata. |
| `repl` | Interactive session: evaluates expressions, keeps declarations and side effects available to later inputs, supports multi-line blocks, `.help`, `.clear` and `.exit`. Errors never end the session. |

## Mobile (Android)

`new mobile <name>` scaffolds an Android project. `nxl.toml` carries the app
name, application id, colours, welcome copy, tab labels/assets, initial tab and
platform target. `build` regenerates the Android resources from that section,
compiles every screen, runs Gradle and reports the APK. `dev` builds, installs
and launches on a connected device or emulator.

Requirements: JDK 17+, Android SDK platform/build-tools 35, and (for `dev`) a
device in the `device` state. Rust and the NDK are not required to build a
generated project: it ships prebuilt `x86_64` and `arm64-v8a` NXL runtimes.
Release signing reads `NXL_ANDROID_KEYSTORE`,
`NXL_ANDROID_KEYSTORE_PASSWORD`, `NXL_ANDROID_KEY_ALIAS` and
`NXL_ANDROID_KEY_PASSWORD` from the environment; without them the release
artifact is unsigned. No credential is written into the project. Full details,
including limitations and error codes, are in [Mobile](25-mobile.md).

## Package manager (NXP)

| Command | What it does |
| --- | --- |
| `pkg install [--offline]` | Resolve dependencies and write `nxl.lock`. |
| `pkg add <name> [req] [--registry <dir>]` | Add a dependency. With `--registry` the package must exist there and satisfy the requirement, otherwise the command fails and writes nothing. |
| `pkg add <name> --path <dir>` | Add a local path dependency. Validated on disk, resolved (including its own path dependencies), cached and locked with a SHA-256 checksum. |
| `pkg remove <name>` | Remove a dependency. Fails if it is not declared. |
| `pkg update` | Re-resolve dependencies. |
| `pkg tree` | Dependency tree with resolved versions. |
| `pkg audit` | Match the lockfile against the local advisory set. |
| `pkg search [term] [--registry <dir>]` | Search local dependencies, the package cache, and a local registry directory. |
| `pkg info <name>` | Requirement, resolved version and source of a dependency. |
| `pkg publish [--registry <dir>]` | Publish the project into a registry directory: manifest, sources, per-file SHA-256 checksums and an `index.json`. Defaults to `$NXL_REGISTRY` or `~/.nexora/registry`. |
| `pkg install [--registry <dir>]` | Resolve dependencies and write `nxl.lock`, using the given registry when provided. |

Short forms work too: `install`, `add`, `remove`, `update`, `audit`, `tree`,
`search`, `info`, `publish`.

## Database (NDB)

`db:create`, `db:drop [--yes]`, `db:migrate`, `db:rollback`, `db:refresh [--yes]`,
`db:fresh [--seed] [--yes]`, `db:seed`, `db:status`, `db:audit`.

`db:drop`, `db:fresh`, and `db:refresh` destroy data, so they require
confirmation: an interactive prompt, or `--yes` for scripts and CI (NXL12010).

## Scaffolding

`make:model`, `make:migration`, `make:seeder`, `make:factory`.

## Web, tooling and interoperability

| Command | Status |
| --- | --- |
| `serve` | NUF server. Serves the routes the project declares in `nxl.toml` (`[[route]]`), with `/openapi.json` describing them. A project with no routes gets the demonstration routes. Renders the declared layout and 404 page; every response carries `nosniff`, `referrer-policy: strict-origin-when-cross-origin` and `x-frame-options: DENY`. Not a development server with hot reload — that is `dev`. |
| `lsp` | Language server (NLS). The CLI entry point validates initialisation. |
| `ide` | Print the IDE capability sources. |
| `bridge` | Print FFI bridge status. |
| `bench` | Run smoke benchmarks. |

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | Failure — a program error, a failed check, or a refused operation |
| 2 | Usage error (missing or invalid arguments) |

Commands that cannot do what was asked exit non-zero. Unsupported operations
print an explanation and fail; they never report success for something that did
not happen.

## Project layout created by `new console`

```text
<name>/
  nxl.toml          [package] metadata and the entry point
  src/main.nxl      program entry point
  tests/basic.nxl   compiled and run by `test`
  README.md         project notes
  .gitignore
```

## Project layout created by `new web`

```text
<name>/
  nxl.toml              [package] metadata plus the [[route]] table served by `serve`
  pages/                one .nxlhtml template per route
  layouts/main.nxlhtml  nav, brand, <title>, footer — wrapped around every page
  src/main.nxl          boot sequence; prints the log the page is built from
  src/app/config.nxl    name, tagline and features
  src/app/routes.nxl    the routes the dev server exposes
  src/app/state.nxl     AppState, the boot state machine
  public/index.nxl      the dev-server landing page, written in NXL
  public/styles.css     page, card, form and navigation styling
  public/bootstrap.js   turns the program's print() output into the page
  public/favicon.svg    the mark used by the page and the tab
  tests/basic.nxl       compiled and run by `test`
  README.md             project notes, including how to add a page
  .gitignore
```

Two commands serve it, and they are different servers:

- `nxlc serve` renders the `[[route]]` table in `nxl.toml` — the pages in
  `pages/`, with the declared layout and 404 — as finished HTML.
- `nxlc dev` serves the browser program: it compiles `src/` to JavaScript,
  serves `public/`, and reloads on edits. It falls back to its own generated
  page for projects that have no `public/` directory. See
  [repl-and-dev.md](repl-and-dev.md) for what the dev server does and does not
  do.

Both accept `[web] host`/`port` from `nxl.toml`; `serve` binds `127.0.0.1:8080`
and `dev` `127.0.0.1:3000` by default.

`--target` and `--template` are the same flag. The word changed because a
`console` project is not the only shape a program can take; both spellings are
accepted and neither will be removed.