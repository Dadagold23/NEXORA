# NEXORA / NXL — Linux and macOS

This package contains the NXL compiler for the platform named in the archive
name (`linux-x64`, `linux-arm64`, `macos-arm64`, `macos-x64`).

| File               | Purpose                                                |
| ------------------ | ------------------------------------------------------ |
| `nxlc`             | The compiler. No Rust toolchain required.               |
| `install.sh`       | Installer (checksum verification, user install, PATH)  |
| `checksums.sha256` | SHA-256 of every packaged file                         |
| `examples/main.nxl`| Runnable hello-world                                   |
| `LICENSE`, `CHANGELOG.md`, `SECURITY.md` | Legal and project information         |

## Install

```bash
tar -xzf NEXORA-<version>-<platform>.tar.gz
cd NEXORA-<version>-<platform>   # the archive contains these files at the top level
./install.sh
```

Check what it would do without touching anything:

```bash
./install.sh --dry-run
```

The installer verifies `nxlc` against `checksums.sha256` (using `sha256sum`, or
`shasum -a 256` on macOS) before copying anything, installs to
`~/.nexora/bin`, writes `~/.nexora/env`, adds a loader line to `~/.profile`,
and then runs the installed compiler to prove it works. It refuses to run if
the archive does not match your operating system or CPU architecture.

If you prefer a different location:

```bash
./install.sh --prefix "$HOME/.local/bin"
```

## Verify

Open a new terminal (or run `source ~/.nexora/env`), then:

```bash
nxlc --version
nxlc doctor
nxlc run ./examples/main.nxl
```

## Uninstall

```bash
./install.sh --uninstall
```

Remove any leftover PATH line for the install directory from your shell
profile if you changed the prefix.

## macOS notes

- These builds are **not signed or notarised**. macOS Gatekeeper may therefore
  refuse to run the compiler on first launch. Until signing is available, run
  it from a terminal (`./nxlc`) or clear the quarantine attribute for your own
  copy with `xattr -d com.apple.quarantine ./nxlc`.
- Apple Silicon (`macos-arm64`) and Intel (`macos-x64`) are separate packages.

## Linux notes

- The binary is built with the Rust standard library's default target and has no
  runtime dependencies beyond glibc. It is produced on a specific CI image; we
  have not yet verified it across a range of distributions, so report any
  loader problem as a bug with your distribution and glibc version.

## Known limitations

- `nxlc doctor` reports the optional Node.js runtime: without it the
  JavaScript backend is unavailable and programs run on the VM (`nxlc run`),
  which is fully supported.
- Concurrency (`spawn`/`await`/`channel`) runs on the VM only; the JavaScript
  backend refuses it with a clear error.