# Onboarding and release verification

This records what a new user actually experiences when installing a NEXORA
release, and how that is verified rather than assumed.

## How this is verified

`scripts/verify-install.ps1` (Windows) and `scripts/verify-install.sh`
(Linux/macOS) take a **release archive**, extract it, install it into a
temporary prefix, and then run the documented onboarding steps with a cleaned
PATH so nothing leaks in from the developer's shell. The checklist covers:

1. the archive extracts
2. the installer verifies the SHA-256 checksum
3. the installer installs
4. the compiler launches and reports its version
5. `doctor` reports accurate diagnostics
6. the bundled example runs
7. invalid source fails with a non-zero exit code
8. project creation works in a clean directory, and the generated project runs
9. paths containing spaces work
10. a missing file is diagnosed rather than reported as success
11. reinstalling over an existing installation is safe
12. uninstall removes the installation (skipped with `--real-install`)

Run it yourself:

```powershell
# Windows
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-install.ps1
```

```bash
# Linux / macOS
sh scripts/verify-install.sh
```

The release workflow runs this against every archive it produces, before that
archive is published.

## Measured run (2026-10-15)

**This was not a clean machine.** It is a Windows 11 development workstation
(Rust toolchain present, Node.js installed, warm filesystem cache, Windows
Defender scanning newly written executables). Treat the numbers as an upper
bound on a typical developer machine, not as the onboarding target.

| Step                                   | Result | Time  |
| -------------------------------------- | ------ | ----- |
| archive extracts                       | PASS   | 4.3s  |
| installer verifies the checksum        | PASS   | 5.9s  |
| installer installs                     | PASS   | 44.7s |
| compiler launches and reports version  | PASS   | 10.0s |
| `doctor` reports accurate diagnostics  | PASS   | 9.5s  |
| bundled example runs                   | PASS   | 4.5s  |
| invalid source rejected (exit 1)       | PASS   | 13.3s |
| project creation in a clean directory  | PASS   | 7.4s  |
| paths containing spaces                | PASS   | 3.7s  |
| missing file diagnosed                 | PASS   | 1.5s  |
| reinstall is safe                      | PASS   | 23.0s |
| uninstall removes the installation     | PASS   | 27.5s |
| **total**                              | **12/12** | **165s** |

Where the time goes: each step spawns a fresh PowerShell or compiler process,
and Defender re-scans each newly written `nxlc.exe`. The 44.7s "installs" step
is dominated by that first scan, not by the copy.

## What is still missing for the 20-minute target

The target in `re_prompt.txt` is "a beginner installs the toolchain and runs a
basic application within 20 minutes on a compatible computer with internet
access". To claim it, the following must still happen and be recorded:

- **A genuinely clean machine** - a fresh VM or a new user profile with no
  Rust toolchain, no Node.js, and a fresh download. This repository cannot
  produce that evidence from a developer workstation.
- **The download step timed separately**, including the archive size and the
  time to fetch it on a normal connection. The archives are roughly 1.5 MB
  compressed, but no measurement has been taken.
- **Linux and macOS runs** of `verify-install.sh`; those need their platform's
  CI runner and have not been executed yet.
- **The web application path**: `nexora dev` and `nexora new web` are
  implemented and tested on Windows only. The Linux and macOS runners have not
  exercised them yet.

## Conclusion

The Windows x64 release passes the full checklist on a developer machine in
about 2 minutes 45 seconds of verification work. The onboarding target is
plausible but **not demonstrated**, and no claim is made that it has been met
until a clean-machine run is recorded above.