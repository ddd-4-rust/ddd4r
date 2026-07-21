//! Architecture engine integration contracts.

use std::path::PathBuf;

use ddd4r_ddd_rules::{ArchitectureChecker, ArchitecturePolicy};

fn fixture_manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/invalid/Cargo.toml")
}

#[test]
fn cargo_metadata_and_syn_detect_real_dependency_violations() {
    let report = ArchitectureChecker::new(ArchitecturePolicy::clean())
        .check(fixture_manifest())
        .unwrap();

    assert_eq!(report.checked_packages, 5);
    assert!(!report.is_compliant());
    assert!(report.violations.iter().any(|violation| {
        violation.rule == "domain-framework-independence"
            && violation.package == "shop-domain"
            && violation.dependency.as_deref() == Some("axum")
    }));
    assert!(report.violations.iter().any(|violation| {
        violation.rule == "layer-dependency-direction"
            && violation.package == "shop-application"
            && violation.dependency.as_deref() == Some("shop-infrastructure")
    }));
}

#[test]
fn cola_policy_uses_the_same_inward_dependency_direction() {
    let report = ArchitectureChecker::new(ArchitecturePolicy::cola())
        .check(fixture_manifest())
        .unwrap();

    assert!(
        report
            .violations
            .iter()
            .any(|violation| violation.rule == "layer-dependency-direction")
    );
}
