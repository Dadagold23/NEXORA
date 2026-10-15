# Release checklist

Run through this before calling a release candidate ready. Every item either
passes, or is explicitly marked as not done — nothing is assumed.

## 1. Gates (automated)

| Check | Command | Must be |
| --- | --- | --- |
| Formatting | `cargo fmt -- --check` | clean |
| Lints | `cargo clippy --all-targets -- -D warnings` | clean |
| Tests | `cargo test` | all passing, no failures |
| Packaging | `scripts/package-release.ps1` / `.sh` | exits 0 |

`scripts/package-release.{ps1,sh}` runs the first three itself and refuses to
produce an archive if any fail. The Unix script also refuses to run on a host
that does not match the requested target, so a binary can never be published
under the wrong platform name.

## 2. The artifact itself (automated)

After the archive exists, run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-install.ps1
```

```bash
sh scripts/verify-install.sh
```

All twelve steps must pass: extract, checksum verification, install, version,
`doctor`, bundled example, invalid source rejected, project creation and run,
paths with spaces, missing-file diagnosis, safe reinstall, uninstall.

The release workflow performs the same check on every runner **before**
uploading, so an artifact that cannot be installed or executed is never
published.

## 3. Manual review

- [ ] `CHANGELOG.md` matches what actually changed.
- [ ] `docs/02-installation.md` and `docs/03-cli-reference.md` describe the
      commands and installers that ship in this version.
- [ ] `docs/platform-support.md` has no claim stronger than the evidence.
- [ ] No credentials, tokens or signing material anywhere in the tree or in
      the release assets.
- [ ] No absolute paths from a build machine leak into shipped files.
- [ ] Script files have no BOM and use LF endings (`bash -n` passes).

## 4. Clean-machine test (not yet done)

Required before the onboarding target can be claimed. Follow
[`docs/onboarding-report.md`](onboarding-report.md), which lists exactly what is
still missing. In short: a machine with no Rust toolchain, no Node.js and a
fresh download, with the download step timed separately.

## 5. Release publication

- [ ] Tag matches the version in `compiler/Cargo.toml` and `CHANGELOG.md`.
- [ ] `ci.yml` passed on `main`.
- [ ] `release.yml` produced one archive per target.
- [ ] `SHA256SUMS` and `MANIFEST.md` are attached and match the archives.

## Rollback plan

**Before publication** — nothing has shipped, so the rollback is simply to
delete the draft release and the tag:

```bash
git tag -d v0.1.0
git push origin :refs/tags/v0.1.0
```

**After publication** — users install to their own directories
(`%LOCALAPPDATA%\NEXORA\bin` or `~/.nexora/bin`) and no system state was
changed, so a broken release can be withdrawn without touching users' machines:

1. Mark the release as a prerelease or delete it. Downloaders stop getting it;
   existing installations keep working.
2. Publish a hotfix tag (for example `v0.1.1`) containing only the fix.
3. Communicate in the release notes what changed and whether users must
   reinstall.
4. Because installs are user-scoped and self-contained, the recovery path is
   simply: download the fixed archive, run the installer over the existing
   installation (reinstall is verified safe), or run `install.ps1 -Uninstall` /
   `install.sh --uninstall` and then install the fixed version.

**If an installer itself is faulty**: it only ever writes to its own install
directory and its own PATH entry, so uninstall removes everything it created.
No user project is touched by install, upgrade or uninstall.

**Data recovery**: not applicable — the toolchain stores no user data. Project
state lives in the project directory, which installers never modify.