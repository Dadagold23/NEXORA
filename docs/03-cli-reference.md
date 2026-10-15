# CLI Reference

Every command is available as both `nxlc` and `nexora` — they are the same
binary under two names. `nxlc` is the compiler spelling, `nexora` the
project-facing one.

## Project commands

| Command | What it does |
| --- | --- |
| `new [<target>] <name>` | Create a project from a target. `new hello`, `new console hello`, `new --template console hello`, `new --template=console hello`, `new --target console hello` and `new --target=console hello` are equivalent. Targets: `console`, `web`. Dependencies declared by the new manifest are resolved and locked before the command returns. |
| `init` | Create the same project layout in the current directory. |
| `run [file]` | Run a project entry point or a single `.nxl` file on the VM. |
| `build [--release]` | Compile the project into `build/`. |
| `check [file]` | Parse and type-check without producing output. |
| `test [filter]` | Parse, type-check and run every `.nxl` file in `tests/`. There is no test-block syntax yet. |
| `clean` | Remove build output. |
| `fmt [file]`, `lint [file]` | Normalise whitespace; lint delegates to check. |
| `dev [--port N] [--host H:P] [--no-watch]` | Start the development server: compiles the project to JavaScript, serves it with live reload over SSE, and shows a compile-error page that recovers on the next fix. Binds `127.0.0.1` by default. |
| `doctor` | Real environment diagnostics: compiler version and architecture, Node.js availability, `nxl.toml` and its declared entry point. |
| `debug <file> [--break=N]` | Statement trace with breakpoint counts. |
| `docs` | Print the documentation map. |
| `platforms` | Print the platform support summary. |
| `version`, `--version`, `help` | Toolchain metadata. |
| `repl` | Interactive session: evaluates expressions, keeps declarations and side effects available to later inputs, supports multi-line blocks, `.help`, `.clear` and `.exit`. Errors never end the session. |

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

`db:create`, `db:drop`, `db:migrate`, `db:rollback`, `db:refresh`,
`db:fresh [--seed]`, `db:seed`, `db:status`, `db:audit`.

## Scaffolding

`make:model`, `make:migration`, `make:seeder`, `make:factory`.

## Web, tooling and interoperability

| Command | Status |
| --- | --- |
| `serve` | NUF demonstration server: exposes `/openapi.json`. Not a development server with hot reload. |
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
  nxl.toml              [package] metadata; entry is src/main.nxl, target js
  src/main.nxl          boot sequence; prints the log the page is built from
  src/app/config.nxl    name, tagline and features
  src/app/routes.nxl    the routes the dev server exposes
  src/app/state.nxl     AppState, the boot state machine
  public/index.nxl      the page, written in NXL and rendered on the server
  public/styles.css     page and splash styling
  public/bootstrap.js   turns the program's print() output into the page
  public/favicon.svg    the mark used by the page and the tab
  tests/basic.nxl       compiled and run by `test`
  README.md             project notes
  .gitignore
```

Serve it with `nxlc dev`, which serves `public/` and falls back to its own
generated page for projects that have no `public/` directory. See
[repl-and-dev.md](repl-and-dev.md) for what the dev server does and does not
do.

`--target` and `--template` are the same flag. The word changed because a
`console` project is not the only shape a program can take; both spellings are
accepted and neither will be removed.