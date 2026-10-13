# NXP — Package Manager (P13)

Implemented in `compiler/src/pkg.rs` with CLI subcommands:

```bash
nxl install [--offline]
nxl add <pkg> [req]        # req: exact "1.2.3", ^1.2, ~1.2, >=1.2, or path:.. / git:URL[#tag]
nxl remove <pkg>
nxl update
nxl audit                  # local advisory match on lockfile
nxl tree                   # dependency tree
nxl publish                # NOT IMPLEMENTED (live registry)
```

- `nxl.toml` `[dependencies]` + optional `[dev-dependencies]` + `[workspace]`
- `nxl.lock` deterministic lockfile (name/version/source/checksum), FNV-1a content checksums
- Flat + transitive resolution with cycle/conflict errors (NXL13005)
- Local cache at `.nxl-cache`; offline mode verifies from cache
- NO live registry, NO cryptographic signing — honestly deferred (P17+/P28+)
