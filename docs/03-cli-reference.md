# CLI Reference

All commands are dispatched by `nxlc` (`cargo run -- <cmd>`).

| Command | What it does (status) |
| --- | --- |
| `new <name>` | Scaffold a project (P12) |
| `init` | Scaffold in the current dir (P12) |
| `run [file]` | Run via JS runtime bridge (P12) |
| `build [--release]` | Compile project (P12) |
| `check` | Type-check via NIR lowering (P12) |
| `fmt` | Normalise whitespace (P12, minimal) |
| `lint` | Delegates to check (P12) |
| `clean` | Remove build/ (P12) |
| `test [filter]` | Discover *_test.nxl and compile-check (P12) |
| `repl` | NOT IMPLEMENTED |
| `debug <file> [--break=N]` | Statement trace + breakpoint hits (P21) |
| `docs` | Docs pointer (P26) |
| `doctor` | Toolchain diagnostics (P12) |
| `install [--offline]`, `add`, `remove`, `update`, `audit`, `tree`, `publish` | NXP package manager (P13) |
| `db:create` `db:drop` `db:migrate` `db:rollback` `db:refresh` `db:fresh [--seed]` `db:seed` `db:status` `db:audit` | NDB (P14) |
| `make:model` `make:migration` `make:seeder` `make:factory` | NDB scaffolds (P14) |
| `serve` | NUF demo HTTP server, exposes `/openapi.json` (P15) |
| `bridge` | List FFI bridge status (P17) |
| `platforms` | Print honest platform matrix (P19) |
| `lsp` | Start NLS on stdio (P20) |
| `ide` | Print NIDE capability sources (P22) |
| `bench` | Run smoke benchmarks (P24) |
| `version`, `help` | Toolchain metadata |

Flags: `--offline` for install; `[--seed]` for db:fresh; `--break=N` for debug; `--release` for build.
Exit codes: 0 success, 1 failure, 2 usage error.
