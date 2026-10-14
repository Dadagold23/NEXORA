# NEXORA / NXL — Windows x64

This package contains the NXL compiler for Windows 10/11 x64.

| File             | Purpose                                              |
| ---------------- | ---------------------------------------------------- |
| `nxlc.exe`       | The compiler. No Rust toolchain or Python required.  |
| `install.ps1`    | Installer (checksum verification, user install, PATH) |
| `install.bat`    | Same installer, for Command Prompt                    |
| `checksums.sha256` | SHA-256 of every packaged file                      |
| `examples/main.nxl` | Runnable hello-world                              |

## Install

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1
```

Or from Command Prompt:

```bat
install.bat
```

The installer verifies `nxlc.exe` against `checksums.sha256` before copying
anything, installs to `%LOCALAPPDATA%\NEXORA\bin`, adds that directory to
**your** user PATH (never the system PATH), and then runs the installed
compiler to prove it works. If any step fails, nothing is installed.

Check what it would do without touching anything:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1 -DryRun
```

## Verify

Open a **new** terminal, then:

```powershell
nxlc --version
nxlc doctor
nxlc run .\examples\main.nxl
```

## Uninstall

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1 -Uninstall
```

## Notes and limitations

- Windows x64 only. ARM64, macOS and Linux are separate packages.
- `nxlc doctor` reports the optional Node.js runtime: without it the
  JavaScript backend is unavailable, and programs run on the VM instead
  (`nxlc run`), which is fully supported.
- Concurrency (`spawn`/`await`/`channel`) runs on the VM only; the JavaScript
  backend refuses it with a clear error.
- Gatekeeper/notarization notes apply to macOS only and do not affect this
  package; this build is not code-signed, so Windows SmartScreen may warn on
  first run.