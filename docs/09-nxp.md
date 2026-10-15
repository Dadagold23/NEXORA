# NXP — Package Manager (P13)

Implemented in `compiler/src/pkg.rs` with CLI subcommands (both `nxl pkg <sub>`
and the short top-level forms work):

```bash
nxl pkg install [--offline] [--registry DIR]   # also: nxl install
nxl pkg add <name> [req] [--registry DIR]       # registry dependency
nxl pkg add <name> --path DIR                   # local path dependency
nxl pkg remove <name>                           # also: nxl remove
nxl pkg update
nxl pkg audit                 # local advisory match against the lockfile
nxl pkg tree                  # dependency tree with resolved versions
nxl pkg search [term] [--registry DIR]
nxl pkg info <name> [--registry DIR]
nxl pkg publish [--registry DIR]   # publish into a registry directory
```

The registry defaults to `$NXL_REGISTRY`, else `~/.nexora/registry`.

## The local registry

There is no hosted registry. Publishing writes a real, inspectable package into
a directory:

```text
<registry>/index.json                       # name, version, description, checksum
<registry>/<name>/<version>/package.json    # manifest and metadata
<registry>/<name>/<version>/src/*.nxl      # the sources
<registry>/<name>/<version>/checksums.sha256
```

Because it is a directory, it can be vendored, archived, committed alongside a
project or shared out of band. A full round trip:

```bash
# publish a library
cd mylib && nxl pkg publish

# use it
cd ../consumer
nxl pkg add mylib "^0.1" --registry ~/.nexora/registry
nxl pkg install --registry ~/.nexora/registry
```

`add` verifies the package exists and satisfies the requirement **before**
writing anything, then installs the published sources (not a placeholder) and
records a SHA-256 checksum in `nxl.lock`.

## What works today

- **Path dependencies are fully functional**: `nxl pkg add <name> --path <dir>`
  validates the directory, checks its `nxl.toml` name matches, resolves it
  (including its own path dependencies, which are relative to *that* package),
  copies the source into `.nxl-cache`, and records a SHA-256 checksum in
  `nxl.lock`.
- **Publishing and installing from a local registry** works end to end, with
  checksums and an index.
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

- **A dependency that cannot be resolved is never recorded.** `nxl pkg add`
  checks the registry first and writes nothing when the package is missing,
  out of range, or the registry does not exist.
- `nxl pkg publish` fails (NXL13011) when there is no manifest or nothing to
  publish, instead of reporting a success.
- `nxl pkg search`/`info` say that the registry is local and not hosted.
- A path dependency that does not exist, is not a package, or whose manifest
  name does not match is rejected (NXL13009) **before** anything is written.
- `nxl pkg remove` fails if the dependency is not declared (NXL13006).

## Checksums

`nxl.lock` records a **SHA-256** digest of each package source, and each
published file gets its own entry in the registry's `checksums.sha256`. The
earlier FNV-1a stand-in could not detect tampering, which defeats the purpose of
an integrity hash; it was replaced (verified against the FIPS 180-4 vectors).

## Known limitations

- **No hosted registry.** Publishing targets a directory; there is no service,
  no authentication, no remote publishing and no package signing.
- Git dependencies are recorded but their source is a placeholder: nothing is
  cloned or fetched.
- `nxl audit` matches a local advisory set only; there is no advisory feed.
- Registry installs copy the sources into the cache; there is no verification
  that a cached package still matches the checksum it was installed with.
- No workspaces are materialised yet (`[workspace]` is parsed by
  `pkg::WorkspaceConfig` but not acted on by the CLI).