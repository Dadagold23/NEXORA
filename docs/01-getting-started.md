# Getting Started

## Install

Download the release package for your platform and run its installer — see
[`02-installation.md`](02-installation.md). After installing you have `nxlc`
(the compiler) and can use the shorter `nexora` spelling for project work.

## Build from source

Requirements: Rust stable.

```bash
cd compiler
cargo build --release
cargo test
```

## Create and run a project

```bash
nexora new console hello
cd hello
nexora run          # Hello, NXL!
nexora check        # type-check without output
nexora build --release
nexora test         # compile and run everything in tests/
nexora doctor       # toolchain diagnostics
```

`nexora` and `nxlc` are the same tool: `nexora` is the project-facing spelling
and `nxlc` the compiler spelling. Both accept every command.

There are two targets:

```bash
nexora new console hello   # a program that prints to the terminal
nexora new web site        # a web application with a landing page
cd site && nexora dev      # http://127.0.0.1:3000
```

`--target` and `--template` name the same thing, so `new --template web site`
works too. `new` resolves and locks the new project's dependencies before it
returns, so the project is ready to run.

A web project is served by `nexora dev`, which compiles it to JavaScript, serves
the landing page in `public/`, and reloads the browser when you edit a source
file. The program runs in the browser; NXL has no server-side HTTP types yet.
See [repl-and-dev.md](repl-and-dev.md) for exactly what the dev server does.

## Project layout created by `nexora new console`:

```text
hello/
  nxl.toml          # [package] metadata and the entry point
  src/main.nxl      # program entry point
  tests/basic.nxl   # compiled and run by `nexora test`
  README.md         # project-specific notes
  .gitignore
```

`nxl init` creates the same layout in the current directory using the current
directory's name.

## Project layout created by `nexora new web`

```text
site/
  nxl.toml              # [package] metadata; entry is src/main.nxl
  src/main.nxl          # boot sequence; prints the log the page is built from
  src/app/config.nxl    # name, tagline and features
  src/app/routes.nxl    # the routes the dev server exposes
  src/app/state.nxl     # AppState, the boot state machine
  public/index.html     # landing page, including the splash screen
  public/styles.css     # page and splash styling
  public/bootstrap.js   # turns the program's print() output into the page
  public/favicon.svg    # the mark used by the page and the tab
  tests/basic.nxl       # compiled and run by `nexora test`
  README.md             # project-specific notes
  .gitignore
```

Layout created by `nxl init`:

After building, artifacts land in `build/` (gitignored):

```text
hello/build/            # build artifacts
```

Next: [CLI Reference](03-cli-reference.md).
