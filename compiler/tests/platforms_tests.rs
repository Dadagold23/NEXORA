use nxl_compiler::platforms;

#[test]
fn no_target_is_tier1_without_complete_evidence() {
    for target in platforms::default_matrix() {
        if target.tier() == platforms::Tier::Tier1 {
            assert!(
                target.evidence.complete(),
                "{target:?} claims Tier 1 without evidence"
            );
        }
    }
}

#[test]
fn only_windows_has_evidence_today() {
    let matrix = platforms::default_matrix();
    let windows = matrix.iter().find(|t| t.name.contains("Windows")).unwrap();
    assert_eq!(windows.tier(), platforms::Tier::Tier3);
    for t in &matrix {
        if t.name != "Windows x86_64" {
            assert_eq!(
                t.tier(),
                platforms::Tier::Tier4,
                "{t:?} unexpectedly promoted"
            );
        }
    }
}

#[test]
fn tier_thresholds_follow_evidence_strength() {
    let mut t = platforms::default_matrix()[0].clone();
    t.evidence = platforms::Evidence::default();
    assert_eq!(t.tier(), platforms::Tier::Tier4);
    t.evidence.build_test = true;
    assert_eq!(t.tier(), platforms::Tier::Tier3);
    t.evidence.runtime_test = true;
    t.evidence.integration_test = true;
    assert_eq!(t.tier(), platforms::Tier::Tier2);
    t.evidence.packaging_test = true;
    t.evidence.installation_test = true;
    t.evidence.upgrade_test = true;
    t.evidence.performance_test = true;
    assert_eq!(t.tier(), platforms::Tier::Tier1);
}

#[test]
fn record_evidence_updates_target_store() {
    let mut store = std::collections::BTreeMap::new();
    platforms::record_evidence(&mut store, "Linux x86_64", "build", true);
    platforms::record_evidence(&mut store, "Linux x86_64", "runtime", true);
    let e = &store["Linux x86_64"];
    assert!(e.build_test && e.runtime_test && !e.integration_test);
}

#[test]
fn rendered_matrix_is_honest() {
    let out = platforms::render_matrix(&platforms::default_matrix());
    assert!(out.contains("Tier 4 — Planned"));
    assert!(out.contains("Tier 3 — Experimental"));
    // No target is advertised as production-ready
    assert!(
        !out.contains("Tier 1 — Production\n  target"),
        "matrix header row must not claim Tier 1"
    );
}
