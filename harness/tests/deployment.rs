#[test]
fn shipped_service_only_enables_available_skills() {
    let unit = include_str!("../../scripts/systemd/scoracle-cognition.service");
    let configured = unit
        .lines()
        .find_map(|line| line.strip_prefix("Environment=COGNITION_STAGES="))
        .unwrap();
    let enabled =
        scoracle_cognition::harness::registration::enabled_from_config(Some(configured)).unwrap();
    assert_eq!(enabled.len(), 7);
    assert!(enabled.contains("classifier"));
    assert!(!enabled.contains("investigate_entity"));
    assert!(!enabled.contains("fixture_boxscore"));
}
