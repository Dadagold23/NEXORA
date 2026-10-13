# Getting Started

Requirements: Rust stable (for the `nxlc` toolchain crate). Python prototype is reference-only.

```bash
cd compiler
cargo build --release
cargo test --workspace
```

Create a project:

```bash
nxlc new hello
cd hello
nxlc run          # executes src/main.nxl through the JS-runtime bridge
nxlc check        # type-check without output
nxlc doctor       # toolchain diagnostics
```

Layout created by `nxl init`:

```text
hello/
  nxl.toml          # [package] + [database] + [web] metadata
  src/main.nxl
  database/migrations/
  database/seeders/
  database/factories/
  build/            # build artifacts (gitignored)
```

Next: [CLI Reference](02-cli-reference.md).
