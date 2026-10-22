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

Module 6 adds Android APK build/package evidence and two prebuilt NXL ABIs
(`x86_64`, `arm64-v8a`), but this P19 matrix is deliberately unchanged: there
is no on-device runtime evidence yet, so Android remains Tier 4 until the
evidence is recorded. Tier 4 here means "not promoted", not "cannot build".
See [Mobile](25-mobile.md).

Promotion: CI runners must populate `platforms::record_evidence(...)`. The `platforms_tests` suite enforces that no target reports Tier 1/2 without meeting the evidence bar.

C-ABI `extern` calls use OS-shipped libraries in the test suite (`kernel32.dll` on Windows, `libc.so.6` on Linux, `libSystem.B.dylib` on macOS) — no bundled native fixtures — so `tests/ffi_tests.rs` exercises real dynamic loading on every platform CI runs.
