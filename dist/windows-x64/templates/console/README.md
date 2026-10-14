# PROJECT_NAME

A console application written in NXL, created from the `console` template.

## Run

```bash
nexora run          # or: nxlc run
```

Expected output:

```
Hello, NXL!
```

## Build

```bash
nexora build --release
```

The VM compiles the project to bytecode and caches it under `build/`.

## Test

```bash
nexora test        # or: nxlc test
```

`nxl test` parses and type-checks every `.nxl` file in `tests/`, runs it, and
reports how many passed. NXL has no `test` blocks yet, so checks are ordinary
code that throws when something is wrong — see `tests/basic.nxl`.

## Check

```bash
nexora check
```

## Layout

```
nxl.toml            project manifest (name, version, entry point)
src/main.nxl        program entry point
tests/basic.nxl     tests run by `nxlc test`
```

## Requirements

The compiler binary only. Node.js is optional: with it the JavaScript backend
can also run this project, without it everything runs on the VM.

## Uninstall

Delete the project directory. Nothing is installed system-wide.