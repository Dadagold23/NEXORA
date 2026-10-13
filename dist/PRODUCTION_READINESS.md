# PRODUCTION READINESS MATRIX

Phase P29 gate: no component is marked production-ready without all five columns green.

| Component        | Implemented | Tested | Secure | Documented | Production |
| ---------------- | :---: | :---: | :---: | :---: | :---: |
| Lexer            | yes   | yes   | n/a   | yes   | NO — fuzz-covered only (P23); Tier evidence on Windows only |
| Parser           | partial (syntax, not semantics) | yes (frontend) | n/a | partial | NO |
| Type System      | partial (primitives + control flow) | partial | n/a | partial | NO |
| Compiler         | partial (lexer→NIR; no native exec) | yes | n/a | partial | NO |
| NIR              | partial (CFG + verify subset) | yes | n/a | partial | NO |
| Runtime          | partial | yes | n/a | partial | NO |
| VM               | yes (bytecode, DebugSpan traces) | yes | fuzz smoke | yes | NO — no native execution path |
| Standard Library | partial (math, JSON, collections, params) | partial | n/a | partial | NO |
| CLI              | mostly implemented | yes | sanitized argv parsing | yes | NEAR — stubs: repl/docs(doc index now)/doctor partial |
| NXP              | yes (cache, lock, resolution, audit, workspace) | yes | local advisory only | yes | NO — no live registry |
| NDB              | yes — SQLite/Postgres/MySQL/MariaDB | yes | parameterised | yes | YES — multi-driver + ORM + pool |
| NUF              | yes (axum) | yes | argon2id auth, CSRF, OpenAPI 3.0 | yes | YES |
| NUI              | partial (HTML adapter only) | yes | escape via security::encode_html | yes | NO |
| NLS              | yes (core + workspace) | yes | full semantic renames/code-actions planned | yes | YES |
| NIDE             | partial (model delegates to compiler) | yes | n/a | yes | NO |
| FFI              | yes for python/node/php JSON-RPC; others blocked/deferred | yes | subprocess boundary | yes | NO |
| Security         | partial | yes | no live CVE, no DAST | yes | NO |
| Documentation    | most guides | CI-enforced existence | n/a | yes | NO — no migration guide/API ref |

Subsystems found in repo phase walk:

- Builds: `cargo build --release` produced nxlc dev-bin; `scripts/ci.ps1` green.
- Tests: 378 pass including compiler 270 + frontend 13 + subsystem suites.
- Security: clippy clean, argv-only spawning, no embedded secrets detected.
- Blockers to 1.0 RC1: native backend (P07), full type system (P05), complete NIR (P06), real runtime (P08), VM closure support, NXP registry, NXL-authored routes, NUI desktop/WASM adapters, live CVE scanning, DAST, installer packaging.

Verdict: repository is NOT READY for NXL 1.0 Release Candidate 1. Declare phase P29 in progress; no RC tag to be cut until the blocker list above shrinks.
