# NXP — Package Manager (P13)

Implemented in `compiler/src/pkg.rs` with CLI subcommands (both `nxl pkg <sub>`
and the short top-level forms work):

```bash
nxl pkg install [--offline]   # also: nxl install
nxl pkg add <name> [req]      # registry dependency (see the limitation below)
nxl pkg add <name> --path DIR # local path dependency
nxl pkg remove <name>         # also: nxl remove
nxl pkg update
nxl pkg audit                 # local advisory match against the lockfile
nxl pkg tree                  # dependency tree with resolved versions
nxl pkg search [term]         # local dependencies and cached packages
nxl pkg info <name>           # requirement, resolved version and source
nxl pkg publish               # refused (no registry service)
```

## What works today

- **Path dependencies are fully functional**: `nxl pkg add <name> --path <dir>`
  validates the directory, checks its `nxl.toml` name matches, resolves it
  (including its own path dependencies, which are relative to *that* package),
  copies the source into `.nxl-cache`, and records a SHA-256 checksum in
  `nxl.lock`.
- `nxl.toml` `[dependencies]` + `[dev-dependencies]`; values may be a version
  requirement, `path:<dir>`, or `git:<url>[#tag]`.
- `nxl.lock` is deterministic: name, version, source, SHA-256 checksum.
- Versions accept `1`, `1.2` and `1.2.3` (missing parts default to `0`), plus
  `v` prefixes and `-pre` suffixes. `^`, `~`, `>=` and exact requirements work.
- Resolution is flat + transitive with cycle and version-conflict errors
  (NXL13005).
- Local cache at `.nxl-cache`; `--offline` refuses registry dependencies that
  are not already cached.

## Honesty rules

These are deliberate, and covered by tests:

- **There is no live registry.** Adding a registry dependency records it in
  `nxl.toml` and then **fails with exit code 1** and an explanation, rather
  than reporting a success that never happened.
- `nxl pkg publish` **fails** (NXL13010) instead of printing a
  "not implemented" line and exiting 0.
- `nxl pkg search`/`info` cover local dependencies and the cache, and say so.
- A path dependency that does not exist, is not a package, or whose manifest
  name does not match is rejected (NXL13009) **before** anything is written.
- `nxl pkg remove` fails if the dependency is not declared (NXL13006).

## Checksums

`nxl.lock` records a **SHA-256** digest of each package source. The earlier
FNV-1a stand-in could not detect tampering, which defeats the purpose of an
integrity hash; it was replaced (verified against the FIPS 180-4 vectors).

## Known limitations

- No live registry service, no package signing, no remote publishing.
- Git dependencies are recorded but their source is a placeholder: nothing is
  cloned or fetched.
- `nxl audit` matches a local advisory set only; there is no advisory feed.
- No workspaces are materialised yet (`[workspace]` is parsed by
  `pkg::WorkspaceConfig` but not acted on by the CLI).