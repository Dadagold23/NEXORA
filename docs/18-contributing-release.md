# Contributing & Release

- Branch from main; one phase = one PR.
- Required checks (CI): `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test --workspace`, `cargo build --release` on ubuntu/macos/windows. Local equivalent: `scripts/ci.ps1`.
- Bug = failing test repro first. Real defect discovered in another subsystem? Document the dependency and gate against its own phase before changing it.
- Honest status rule: no "PRODUCTION READY" label without evidence (tiers in `IMPLEMENTATION_STATUS.md`).
- Release: push a `v*` tag — `release.yml` builds matrix artifacts, hashes them into `SHA256SUMS`, and drafts GitHub notes.
- Governance/architecture changes: write an Architecture Change Request in the PR description before code.
