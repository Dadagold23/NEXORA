# Installation

NEXORA/NXL ships as a self-contained release archive per platform. No Rust
toolchain, Python or C compiler is needed to use it.

## Windows (x64)

Download `NEXORA-<version>-windows-x64.zip`, extract it, and run the
installer:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1
```

From Command Prompt:

```bat
install.bat
```

To see what it would do first:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1 -DryRun
```

The installer verifies `nxlc.exe` against `checksums.sha256`, installs to
`%LOCALAPPDATA%\NEXORA\bin`, appends that directory to **your** user PATH (the
system PATH is never touched and nothing is elevated), and then runs the
installed compiler to prove the installation works.

## Linux and macOS

Download the archive matching your platform — `linux-x64`, `linux-arm64`,
`macos-arm64` or `macos-x64` — and run:

```bash
tar -xzf NEXORA-<version>-<platform>.tar.gz
./install.sh
```

```bash
./install.sh --dry-run     # check without changing anything
./install.sh --prefix "$HOME/.local/bin"
```

It verifies the checksum with `sha256sum` (or `shasum -a 256` on macOS),
installs to `~/.nexora/bin`, writes `~/.nexora/env` and adds a loader line to
`~/.profile`, then runs the installed compiler. Open a new terminal, or run
`source ~/.nexora/env`, before using `nexora`.

The installer refuses to run if the archive does not match your operating
system or CPU architecture, so a Linux build cannot be installed on macOS by
mistake.

## Verify the installation

```bash
nexora --version        # or: nxlc --version
nexora doctor           # or: nxlc doctor
nexora new console hello
cd hello
nexora run              # Hello, NXL!
```

`doctor` reports the compiler's version and architecture, whether Node.js is
available (and therefore whether the JavaScript backend can be used), and
whether the current directory is a valid NXL project. Optional gaps are
reported as warnings and do not fail the command.

## `nexora` and `nxlc` are the same tool

`nxlc` is the compiler spelling; `nexora` is the project-facing spelling. Both
accept every command. The installer puts both names on your PATH.

## Uninstall

```powershell
# Windows
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1 -Uninstall
```

```bash
# Linux / macOS
./install.sh --uninstall
```

Uninstall removes only the installed executable and this installer's own PATH
entry. It never touches your projects.

## Build from source

```bash
git clone https://github.com/Dadagold23/NEXORA.git
cd NEXORA/compiler
cargo build --release
cargo test
./target/release/nxlc --version
```

## Platform status

| Target        | Installer | Notes                                                        |
| ------------- | --------- | ------------------------------------------------------------ |
| Windows x64   | yes       | Verified locally on every release                             |
| Linux x64     | yes       | Built and self-tested in CI; not yet tested on a clean machine |
| macOS ARM64   | yes       | Built and self-tested in CI; unsigned, so Gatekeeper may warn |
| macOS x64     | yes       | Built and self-tested in CI; unsigned                        |
| Linux ARM64   | yes       | Experimental                                                 |
| Windows ARM64 | no        | Not built                                                    |

See [platform-support.md](platform-support.md) for the evidence behind each row.

## Troubleshooting

**`nexora` is not recognised.** Open a new terminal; the installer cannot
modify a shell that is already running. On Unix, `source ~/.nexora/env`.

**Windows SmartScreen warns about the binary.** Release binaries are not
code-signed, so SmartScreen may warn on first run. The checksum file is the
integrity control — verify it if you are unsure:

```powershell
Get-FileHash .\NEXORA-<version>-windows-x64.zip -Algorithm SHA256
```

and compare with the published `.sha256`.

**macOS refuses to open the compiler.** The build is not notarised. For your
own copy, clear the quarantine flag:

```bash
xattr -d com.apple.quarantine ./nxlc
```

**`doctor` reports no Node.js.** That is not fatal. Without Node.js the
JavaScript backend is unavailable and programs run on the VM with `nxl run`,
which supports the whole language including concurrency.

**Installation reports a checksum mismatch.** The download is corrupt or was
modified. Nothing was installed. Download it again.