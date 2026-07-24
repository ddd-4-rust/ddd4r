//! Default facade feature contract.

#[test]
fn default_facade_exposes_observability() {
    let config = ddd4r::observability::ObservabilityConfig::default();

    assert_eq!(config.service_name, "ddd4r-application");
    assert_eq!(config.filter, "info");
}
