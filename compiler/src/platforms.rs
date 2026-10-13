//! P19 — Cross-Platform Release Strategy
//!
//! The project refuses to call a target "production-ready" without evidence:
//! build, runtime, integration, packaging, installation, upgrade, and
//! performance tests. This module encodes that policy and reports honest
//! per-target maturity. Nothing here is a substitute for real CI runners on
//! those platforms; targets without evidence stay at the bottom of the matrix.
//!
//! # Current honest state (default report)
//!
//! | Target | Tier | Evidence basis |
//! | --- | --- | --- |
//! | Windows (x86_64-pc-windows-msvc) | Tier 3 — Experimental | Local build + full Rust test suite verified |
//! | Linux | Tier 4 — Planned | No evidence |
//! | macOS | Tier 4 — Planned | No evidence |
//! | Web/WASM | Tier 4 — Planned | Backend blocked on P07 |
//! | Android | Tier 4 — Planned | No evidence |
//! | iOS | Tier 4 — Planned | No evidence |

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Tier1 = 1,
    Tier2 = 2,
    Tier3 = 3,
    Tier4 = 4,
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tier::Tier1 => write!(f, "Tier 1 — Production"),
            Tier::Tier2 => write!(f, "Tier 2 — Supported"),
            Tier::Tier3 => write!(f, "Tier 3 — Experimental"),
            Tier::Tier4 => write!(f, "Tier 4 — Planned"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Evidence {
    pub build_test: bool,
    pub runtime_test: bool,
    pub integration_test: bool,
    pub packaging_test: bool,
    pub installation_test: bool,
    pub upgrade_test: bool,
    pub performance_test: bool,
}

impl Evidence {
    /// A target may not be Tier 1 without every evidence bit set.
    pub fn complete(&self) -> bool {
        self.build_test
            && self.runtime_test
            && self.integration_test
            && self.packaging_test
            && self.installation_test
            && self.upgrade_test
            && self.performance_test
    }

    pub fn score(&self) -> usize {
        [
            self.build_test,
            self.runtime_test,
            self.integration_test,
            self.packaging_test,
            self.installation_test,
            self.upgrade_test,
            self.performance_test,
        ]
        .iter()
        .filter(|&&b| b)
        .count()
    }
}

#[derive(Debug, Clone)]
pub struct Target {
    pub name: &'static str,
    pub evidence: Evidence,
}

impl Target {
    /// Derive the honest tier from evidence. No evidence → Tier 4. Any partial
    /// evidence → Tier 3. Build+runtime+integration at minimum → Tier 2.
    /// Full evidence → Tier 1.
    pub fn tier(&self) -> Tier {
        if self.evidence.complete() {
            Tier::Tier1
        } else if self.evidence.build_test
            && self.evidence.runtime_test
            && self.evidence.integration_test
        {
            Tier::Tier2
        } else if self.evidence.score() > 0 {
            Tier::Tier3
        } else {
            Tier::Tier4
        }
    }
}

/// The default cross-platform support matrix, seeded with real evidence from
/// this repository: only Windows has build+test evidence today.
pub fn default_matrix() -> Vec<Target> {
    vec![
        Target {
            name: "Windows x86_64",
            evidence: Evidence {
                build_test: true,
                runtime_test: true,
                ..Default::default()
            },
        },
        Target {
            name: "Linux x86_64",
            evidence: Evidence::default(),
        },
        Target {
            name: "macOS arm64",
            evidence: Evidence::default(),
        },
        Target {
            name: "Web/WASM",
            evidence: Evidence::default(),
        },
        Target {
            name: "Android",
            evidence: Evidence::default(),
        },
        Target {
            name: "iOS",
            evidence: Evidence::default(),
        },
    ]
}

/// Render the matrix as a human-readable table.
pub fn render_matrix(targets: &[Target]) -> String {
    let mut out = String::from("NEXORA platform support matrix:\n");
    out.push_str("  target              tier                     evidence (n/7)\n");
    for t in targets {
        out.push_str(&format!(
            "  {:<20} {:<24} {}/7\n",
            t.name,
            t.tier().to_string(),
            t.evidence.score()
        ));
    }
    out.push_str("\nNo target is promoted to Tier 1/2 without passing ALL required checks.\n");
    out.push_str("Targets without evidence remain Tier 4 — planned.\n");
    out
}

/// Record build evidence into a per-target store (for CI to fill in).
pub fn record_evidence(
    store: &mut BTreeMap<String, Evidence>,
    target: &str,
    key: &str,
    value: bool,
) {
    let e = store.entry(target.to_owned()).or_default();
    match key {
        "build" => e.build_test = value,
        "runtime" => e.runtime_test = value,
        "integration" => e.integration_test = value,
        "packaging" => e.packaging_test = value,
        "installation" => e.installation_test = value,
        "upgrade" => e.upgrade_test = value,
        "performance" => e.performance_test = value,
        _ => {}
    }
}
