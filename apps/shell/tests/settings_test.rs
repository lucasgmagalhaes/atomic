use shell::settings::PerformanceSettings;

#[test]
fn default_max_panes_matches_the_mockups_largest_preset() {
    assert_eq!(PerformanceSettings::new().max_panes, 6);
}

#[test]
fn clamp_pane_count_caps_at_max_and_floors_at_one() {
    let settings = PerformanceSettings { max_panes: 4 };
    assert_eq!(settings.clamp_pane_count(1), 1);
    assert_eq!(settings.clamp_pane_count(4), 4);
    assert_eq!(settings.clamp_pane_count(6), 4);
    assert_eq!(settings.clamp_pane_count(0), 1);
}

#[test]
fn a_zero_max_still_allows_at_least_one_pane() {
    let settings = PerformanceSettings { max_panes: 0 };
    assert_eq!(settings.clamp_pane_count(6), 1);
}
