# Platform support matrix

What is **actually built and tested**, as of NEXORA/NXL 0.1.0. Nothing is
claimed as supported without a built artifact and a passing test.

| Target            | Binary built | Tested on the artifact | Installer | Status                          |
| ----------------- | ------------ | ---------------------- | --------- | ------------------------------- |
| Windows x64       | yes (local)  | yes                    | yes       | **Supported**                   |
| Linux x64         | CI           | yes (CI, on the runner) | yes       | Supported, pending a clean-machine test |
| macOS ARM64       | CI           | yes (CI, on the runner) | yes       | Supported, pending a clean-machine test |
| macOS x64         | CI           | yes (CI, on the runner) | yes       | Supported, pending a clean-machine test |
| Linux ARM64       | CI, best effort | partially             | yes       | Experimental                    |
| Windows ARM64     | no           | no                     | no        | **Not supported**               |

Evidence: `.github/workflows/ci.yml` runs `fmt`, `clippy` and the test suite on
`ubuntu-latest`, `windows-latest` and `macos-latest`.
`.github/workflows/release.yml` builds each target on its own **native** runner
(`ubuntu-22.04`, `macos-14`, `macos-13`, `windows-latest`), runs that
platform's packaging script, and each script verifies the artifact it is about
to ship: `--version`, `doctor`, the bundled example, a non-zero exit on invalid
source, and a dry run of the platform's installer. Only then is the archive and
its SHA-256 published.

## What "pending a clean-machine test" means

Every platform above has an artifact that was built *and* self-tested by CI on
its own runner, but none of them has yet been installed and run on a clean
machine that is not the build machine. The 20-minute onboarding target in
`re_prompt.txt` is therefore **not yet demonstrated**. Until that test is
recorded, treat the non-Windows platforms as "built and tested in CI" rather
than "ready for users".

## Never faked

- `scripts/package-release.sh` refuses to run on a host that does not match the
  requested target, so a Linux binary can never be published as macOS.
- `install/unix/install.sh` refuses to run when the archive does not match the
  host OS or CPU architecture.
- Nothing in the repository stages a foreign binary: `dist/<platform>/` is
  produced only by a runner for that platform.

## Runtime dependencies

| Dependency    | Required? | Effect if missing                              |
| ------------- | --------- | ---------------------------------------------- |
| Rust toolchain| no        | The release binary is prebuilt                  |
| Node.js       | no        | JS backend unavailable; run programs on the VM  |
| bash + POSIX utils | yes (Unix installs) | `sha256sum` or `shasum` verifies the download |
| glibc (Linux) | pinned to the build image | Not yet verified across distributions  |

## Per-target language support

Language support does not depend on the host platform, but the *backends* do:

| Feature                                   | VM (`nxl run`) | JavaScript backend | Rust backend |
| ----------------------------------------- | -------------- | ------------------ | ------------ |
| Expressions, control flow, `match`        | yes            | yes                | straight-line only |
| Classes, methods, modules                 | yes            | yes                | refused (NXL2007) |
| `try`/`catch`/`finally`, `throw`          | yes            | yes                | yes |
| `while`, `for ... in`                     | yes            | yes                | refused (NXL2007) |
| `spawn`/`await`, channels, actor mailbox  | yes            | refused (NXL2007)  | refused (NXL2007) |
| Arrays, structs, enums                    | yes            | yes                | arrays only  |

## Upgrade policy

Re-running the installer replaces `nxlc.exe` in place and refreshes the PATH
entry; user projects are never modified. Uninstall removes only the installed
executable and the installer's PATH entry.