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

The `console` template is the only one that exists today. `nexora new web ...`
is refused with an explanation, because NXL has no web development server and a
generated project that cannot run would be worse than an error.

`nexora dev` (the proposed web workflow) is likewise refused rather than
silently doing nothing.

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

Layout created by `nxl init`:

After building, artifacts land in `build/` (gitignored):

```text
hello/build/            # build artifacts
```

Next: [CLI Reference](03-cli-reference.md).
