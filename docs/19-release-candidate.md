# NEXORA Release Candidate Policy (P29)

Release train (see `prompt.txt` §41):

```text
NXL 0.5 (stdlib core) -> NXL 0.6 (CLI + NXP) -> NXL 0.7 (DB + Web) -> NXL 0.8 (tooling) -> NXL 0.9 (Beta) -> 1.0 RC
```

Each gate has exit criteria documented in `IMPLEMENTATION_STATUS.md`. 1.0 RC1 cannot be cut until ALL of:

1. Compiler never crashed on fuzz inputs (P23)
2. Test pyramid green (unit/integration/compiler/semantic/runtime/perf/security)
3. `cargo clippy --all-targets -- -D warnings` clean
4. Live `cargo audit` run completed in CI with zero unmitigated findings
5. PRODUCTION_READINESS.md shows no NOT-IMPLEMENTED row in compiler/NDB/NXP/Security
6. examples/{hello,blog,api} verified end-to-end

Current state: items 1, 2, 3, 6 TRUE; item 4 pending first CI run; item 5 FALSE — see `PRODUCTION_READINESS.md` blocker list. No RC tags will be created at this point.
