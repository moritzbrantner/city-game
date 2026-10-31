use three_d_camera::CameraError;
use three_d_core::Vec3;

use super::camera::{apply_render_view, fit_orthographic_camera};
use super::frame::{RendererFrame, RendererSceneNode, renderer_frame};
use super::projection::GameWorldProjection;
use super::scene::{
    append_building_node, append_land_use_node, append_road_nodes, append_water_node,
    append_zone_node,
};
use super::view::{RenderFrameError, RenderView};
use crate::{CitySave, CityScenario};

pub fn build_render_frame(
    scenario: &CityScenario,
    aspect: f32,
) -> Result<RendererFrame, CameraError> {
    let (nodes, fit_points) = scenario_render_parts(scenario);
    finish_frame(nodes, fit_points, aspect)
}

pub fn build_render_frame_with_view(
    scenario: &CityScenario,
    aspect: f32,
    view: RenderView,
) -> Result<RendererFrame, RenderFrameError> {
    let (nodes, fit_points) = scenario_render_parts(scenario);
    finish_frame_with_view(nodes, fit_points, aspect, view)
}

pub fn build_save_render_frame(save: &CitySave, aspect: f32) -> Result<RendererFrame, CameraError> {
    let (nodes, fit_points) = save_render_parts(save);
    finish_frame(nodes, fit_points, aspect)
}

pub fn build_save_render_frame_with_view(
    save: &CitySave,
    aspect: f32,
    view: RenderView,
) -> Result<RendererFrame, RenderFrameError> {
    let (nodes, fit_points) = save_render_parts(save);
    finish_frame_with_view(nodes, fit_points, aspect, view)
}

fn scenario_render_parts(scenario: &CityScenario) -> (Vec<RendererSceneNode>, Vec<Vec3>) {
    let projection = GameWorldProjection::from_scenario(scenario);
    let mut nodes = Vec::new();
    let mut fit_points = Vec::new();

    for road in &scenario.roads {
        append_road_nodes(
            &road.id,
            &road.geometry,
            road.class,
            projection,
            &mut nodes,
            &mut fit_points,
        );
    }
    for building in &scenario.buildings {
        append_building_node(building, projection, &mut nodes, &mut fit_points);
    }
    for water in &scenario.water {
        append_water_node(water, projection, &mut nodes, &mut fit_points);
    }
    for land_use in &scenario.land_use_areas {
        append_land_use_node(land_use, projection, &mut nodes, &mut fit_points);
    }

    (nodes, fit_points)
}

pub(super) fn save_render_parts(save: &CitySave) -> (Vec<RendererSceneNode>, Vec<Vec3>) {
    let projection = GameWorldProjection::from_save(save);
    let mut nodes = Vec::new();
    let mut fit_points = Vec::new();
    let planning = &save.world.planning;

    // Rendering borrows planning semantics directly instead of materializing/cloning
    // the public EffectiveRoad query shape. Nodes are sorted once at the frame boundary.
    planning.visit_effective_roads(&save.scenario, &mut |id, geometry, class| {
        append_road_nodes(id, geometry, class, projection, &mut nodes, &mut fit_points);
    });
    for building in &save.scenario.buildings {
        if planning.is_suppressed(&building.id) {
            continue;
        }
        append_building_node(building, projection, &mut nodes, &mut fit_points);
    }
    for water in &save.scenario.water {
        if planning.is_suppressed(&water.id) {
            continue;
        }
        append_water_node(water, projection, &mut nodes, &mut fit_points);
    }
    for land_use in &save.scenario.land_use_areas {
        if planning.is_suppressed(&land_use.id) {
            continue;
        }
        append_land_use_node(land_use, projection, &mut nodes, &mut fit_points);
    }
    for zone in planning.zones.values() {
        append_zone_node(zone, projection, &mut nodes, &mut fit_points);
    }

    (nodes, fit_points)
}

fn finish_frame(
    mut nodes: Vec<RendererSceneNode>,
    fit_points: Vec<Vec3>,
    aspect: f32,
) -> Result<RendererFrame, CameraError> {
    if !aspect.is_finite() || aspect <= 0.0 {
        return Err(CameraError::InvalidAspect);
    }
    nodes.sort_by(|left, right| left.id.cmp(&right.id));
    let camera = fit_orthographic_camera(&fit_points, aspect)?;
    Ok(renderer_frame(nodes, camera, aspect))
}

fn finish_frame_with_view(
    mut nodes: Vec<RendererSceneNode>,
    fit_points: Vec<Vec3>,
    aspect: f32,
    view: RenderView,
) -> Result<RendererFrame, RenderFrameError> {
    if !aspect.is_finite() || aspect <= 0.0 {
        return Err(CameraError::InvalidAspect.into());
    }
    let view = view.validate()?;
    nodes.sort_by(|left, right| left.id.cmp(&right.id));
    let overview = fit_orthographic_camera(&fit_points, aspect)?;
    let camera = apply_render_view(overview, view)?;
    Ok(renderer_frame(nodes, camera, aspect))
}

#[cfg(test)]
mod tests {
    use geo_core::Geometry;

    use crate::{
        BuildingUse, ExternalRevision, PlannedRoad, PlannedZone, PlanningCommand, RoadClass,
        SCENARIO_SCHEMA_VERSION, ScenarioBuilding, ScenarioProvenance, ScenarioRoad, ZoneKind,
    };

    use super::*;
    use crate::render::RenderViewError;

    fn scenario() -> CityScenario {
        CityScenario {
            schema_version: SCENARIO_SCHEMA_VERSION,
            provenance: ScenarioProvenance {
                source_format: "osm-pbf".to_owned(),
                source_name: "fixture".to_owned(),
                source_sha256: "fixture".to_owned(),
                parser: ExternalRevision {
                    repository: "geo-analysis".to_owned(),
                    revision: "fixture".to_owned(),
                },
            },
            roads: vec![ScenarioRoad {
                id: "imported/way/10".to_owned(),
                source_id: "way/10".to_owned(),
                geometry: Geometry::LineString {
                    coordinates: vec![[8.0, 48.0], [8.001, 48.001]],
                },
                class: RoadClass::Residential,
                name: Some("Imported Street".to_owned()),
                lanes: Some(2),
                max_speed_kph: Some(50),
            }],
            buildings: vec![ScenarioBuilding {
                id: "imported/way/20".to_owned(),
                source_id: "way/20".to_owned(),
                footprint: Geometry::Polygon {
                    coordinates: vec![vec![
                        [8.0, 48.0],
                        [8.001, 48.0],
                        [8.001, 48.001],
                        [8.0, 48.0],
                    ]],
                },
                use_kind: BuildingUse::Residential,
                name: None,
                levels: Some(4),
                height_m: None,
                gross_floor_area_m2: 3_600,
            }],
            water: Vec::new(),
            land_use_areas: Vec::new(),
            transit_anchors: Vec::new(),
        }
    }

    #[test]
    fn frame_uses_canonical_game_semantics_and_shared_camera_contract() {
        let scenario = scenario();
        let first = build_render_frame(&scenario, 16.0 / 9.0).unwrap();
        let second = build_render_frame(&scenario, 16.0 / 9.0).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.nodes[0].id, "imported/way/10/road-segment-0");
        assert_eq!(first.nodes[1].id, "imported/way/20");
        assert!((first.camera.aspect - 16.0 / 9.0).abs() <= f32::EPSILON);

        let json = serde_json::to_value(first).unwrap();
        assert!(json["camera"]["viewMatrix"].is_array());
        assert!(json["camera"]["projectionMatrix"].is_array());
        assert_eq!(json["nodes"][0]["geometry"]["kind"], "box");
    }

    #[test]
    fn default_inspection_view_is_byte_for_byte_the_fitted_overview() {
        let scenario = scenario();
        let overview = build_render_frame(&scenario, 16.0 / 9.0).unwrap();
        let inspected =
            build_render_frame_with_view(&scenario, 16.0 / 9.0, RenderView::overview()).unwrap();
        assert_eq!(overview, inspected);
    }

    #[test]
    fn inspection_view_changes_only_camera_projection() {
        let scenario = scenario();
        let overview = build_render_frame(&scenario, 1.0).unwrap();
        let inspected = build_render_frame_with_view(
            &scenario,
            1.0,
            RenderView {
                pan_x: 0.25,
                pan_y: -0.125,
                zoom: 2.0,
            },
        )
        .unwrap();
        assert_eq!(overview.nodes, inspected.nodes);
        assert_eq!(overview.camera.view_matrix, inspected.camera.view_matrix);
        assert_ne!(
            overview.camera.projection_matrix,
            inspected.camera.projection_matrix
        );
        assert!(
            (inspected.camera.projection_matrix[0] - overview.camera.projection_matrix[0] * 2.0)
                .abs()
                < 1.0e-5
        );
        assert!(
            (inspected.camera.projection_matrix[5] - overview.camera.projection_matrix[5] * 2.0)
                .abs()
                < 1.0e-5
        );
    }

    #[test]
    fn invalid_inspection_view_fails_closed() {
        assert_eq!(
            build_render_frame_with_view(
                &scenario(),
                1.0,
                RenderView {
                    pan_x: 0.0,
                    pan_y: 0.0,
                    zoom: 0.0,
                },
            ),
            Err(RenderFrameError::View(RenderViewError::ZoomOutOfRange))
        );
    }

    #[test]
    fn save_frame_projects_effective_planning_overlay() {
        let mut save = CitySave::new(scenario()).unwrap();
        save.apply_planning(PlanningCommand::SuppressScenarioEntity {
            id: "imported/way/10".to_owned(),
        })
        .unwrap();
        save.apply_planning(PlanningCommand::AddRoad {
            road: PlannedRoad {
                id: "player/road/1".to_owned(),
                geometry: Geometry::LineString {
                    coordinates: vec![[8.002, 48.0], [8.003, 48.001]],
                },
                class: RoadClass::Secondary,
                name: None,
            },
        })
        .unwrap();
        save.apply_planning(PlanningCommand::ZoneArea {
            zone: PlannedZone {
                id: "player/zone/1".to_owned(),
                geometry: Geometry::Polygon {
                    coordinates: vec![vec![
                        [8.002, 48.0],
                        [8.003, 48.0],
                        [8.003, 48.001],
                        [8.002, 48.0],
                    ]],
                },
                kind: ZoneKind::Residential,
            },
        })
        .unwrap();

        let frame = build_save_render_frame(&save, 1.0).unwrap();
        assert!(
            frame
                .nodes
                .iter()
                .all(|node| !node.id.starts_with("imported/way/10/road-segment"))
        );
        assert!(
            frame
                .nodes
                .iter()
                .any(|node| node.id == "player/road/1/road-segment-0")
        );
        assert!(frame.nodes.iter().any(|node| node.id == "player/zone/1"));
    }

    #[test]
    fn save_frame_road_membership_matches_effective_road_query_without_query_materialization() {
        let mut save = CitySave::new(scenario()).unwrap();
        save.apply_planning(PlanningCommand::AddRoad {
            road: PlannedRoad {
                id: "player/road/borrowed".to_owned(),
                geometry: Geometry::LineString {
                    coordinates: vec![[8.002, 48.0], [8.003, 48.001]],
                },
                class: RoadClass::Primary,
                name: Some("Borrowed Road".to_owned()),
            },
        })
        .unwrap();

        let expected = save
            .effective_roads()
            .into_iter()
            .map(|road| road.id)
            .collect::<std::collections::BTreeSet<_>>();
        let frame = build_save_render_frame(&save, 1.0).unwrap();
        let actual = frame
            .nodes
            .iter()
            .filter_map(|node| {
                node.id
                    .split_once("/road-segment-")
                    .map(|(id, _)| id.to_owned())
            })
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(actual, expected);
    }

    #[test]
    fn invalid_viewport_aspect_fails_closed() {
        assert_eq!(
            build_render_frame(&scenario(), 0.0),
            Err(CameraError::InvalidAspect)
        );
    }
}
