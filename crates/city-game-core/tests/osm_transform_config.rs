use city_game_core::OsmScenarioTransformConfig;

#[test]
fn checked_in_road_layout_profile_matches_the_canonical_profile() {
    let config: OsmScenarioTransformConfig = serde_json::from_str(include_str!(
        "../../../fixtures/osm-road-layout-only-transform.json"
    ))
    .unwrap();

    assert_eq!(config, OsmScenarioTransformConfig::road_layout_only());
    assert_eq!(config.validate(), Ok(()));
}
