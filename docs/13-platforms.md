# Platform Maturity (P19)

A target may only be **Tier 1 — Production** once it passes ALL of: build test, runtime test, integration test, packaging test, installation test, upgrade test, performance test. Otherwise tiers degrade honestly (partial evidence → Tier 3; build+runtime+integration → Tier 2; nothing → Tier 4).

Current honest matrix (`nxl platforms`):

| Target | Tier | Evidence today |
| --- | --- | --- |
| Windows x86_64 | Tier 3 — Experimental | Rust build + `cargo test` + release build verified locally |
| Linux x86_64 | Tier 4 — Planned | No evidence; CI in `.github/workflows/ci.yml` |
| macOS arm64 | Tier 4 — Planned | No evidence; CI configured |
| Web/WASM | Tier 4 — Planned | Blocked on backend (P07) |
| Android | Tier 4 — Planned | No evidence |
| iOS | Tier 4 — Planned | No evidence |

Promotion: CI runners must populate `platforms::record_evidence(...)`. The `platforms_tests` suite enforces that no target reports Tier 1/2 without meeting the evidence bar.
