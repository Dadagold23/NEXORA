# Installation

NEXORA/NXL is currently distributed as source via the Rust toolchain crate in `compiler/`.

```bash
git clone <repo> nxl
cd nxl/compiler
cargo build --release

# optional: put it on PATH
cp target/release/nxlc ~/.local/bin/nxl
```

First-run verification:

```bash
nxlc --version     # nxlc 0.1.0
nxlc doctor        # toolchain diagnostics
nxlc new hello
cd hello
nxlc check         # type-checks src/main.nxl through NIR lowering
```

No binary installers/packages are produced yet:

| Format | Status |
| --- | --- |
| Windows installer | NOT IMPLEMENTED |
| Linux .deb/.rpm | NOT IMPLEMENTED |
| macOS pkg/App | NOT IMPLEMENTED |
| Portable archive | NOT IMPLEMENTED |
| WASM distribution | NOT IMPLEMENTED (backend pending) |
