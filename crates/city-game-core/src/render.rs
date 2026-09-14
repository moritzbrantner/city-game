use geo_core::Geometry;
use serde::{Deserialize, Serialize};
use three_d_camera::{CameraError, OrthographicCamera};
use three_d_core::Vec3;

use crate::{
    CitySave, CityScenario, RoadClass, ScenarioBuilding, ScenarioRoad, ZoneKind,
};

pub const THREE_D_LAB_REVISION: &str = "6a18cb2d1fe9efdbae619c144b2180fdeb472172";

const MIN_VERTICAL_SPAN: f32 = 150.0;
const FRAMING_PADDING: f32 = 1.1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RendererCamera {
    pub aspect: f32,
    pub view_matrix: [f32; 16],
    pub projection_matrix: [f32; 16],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RendererGeometry {
    Box { size: [f32; 3] },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RendererTransform {
    pub translation: [f32; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_quaternion: Option<[f32; 4]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RendererSceneNode {
    pub id: String,
    pub geometry: RendererGeometry,
    pub color: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
    pub transform: RendererTransform,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RendererFrame {
    pub camera: RendererCamera,
    pub nodes: Vec<RendererSceneNode>,
}

#[derive(Debug, Clone, Copy)]
struct GameWorldProjection {
    origin_lon: f64,
    origin_lat: f64,
    longitude_meters_per_degree: f64,
    latitude_meters_per_degree: f64,
}

impl GameWorldProjection {
    fn from_scenario(scenario: &CityScenario) -> Self {
        let mut positions = Vec::new();
        collect_scenario_positions(scenario, &mut positions);
        Self::from_positions(&positions)
    }

    fn from_save(save: &CitySave) -> Self {
        let mut positions = Vec::new();
        collect_scenario_positions(&save.scenario, &mut positions);
        for road in save.world.planning.player_roads.values() {
            collect_positions(&road.geometry, &mut positions);
        }
        for zone in save.world.planning.zones.values() {
            collect_positions(&zone.geometry, &mut positions);
        }
        Self::from_positions(&positions)
    }

    fn from_positions(positions: &[[f64; 2]]) -> Self {
        let (origin_lon, origin_lat) = if positions.is_empty() {
            (0.0, 0.0)
        } else {
            let min_lon = positions
                .iter()
                .map(|position| position[0])
                .fold(f64::INFINITY, f64::min);
            let max_lon = positions
                .iter()
                .map(|position| position[0])
                .fold(f64::NEG_INFINITY, f64::max);
            let min_lat = positions
                .iter()
                .map(|position| position[1])
                .fold(f64::INFINITY, f64::min);
            let max_lat = positions
                .iter()
                .map(|position| position[1])
                .fold(f64::NEG_INFINITY, f64::max);
            ((min_lon + max_lon) * 0.5, (min_lat + max_lat) * 0.5)
        };
        let latitude_meters_per_degree = 111_320.0;
        let longitude_meters_per_degree =
            latitude_meters_per_degree * origin_lat.to_radians().cos();
        Self {
            origin_lon,
            origin_lat,
            longitude_meters_per_degree,
            latitude_meters_per_degree,
        }
    }

    fn project(self, position: [f64; 2]) -> [f32; 2] {
        let x = (position[0] - self.origin_lon) * self.longitude_meters_per_degree;
        let z = -(position[1] - self.origin_lat) * self.latitude_meters_per_degree;
        [x as f32, z as f32]
    }
}

pub fn build_render_frame(
    scenario: &CityScenario,
    aspect: f32,
) -> Result<RendererFrame, CameraError> {
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
        append_area_node(
            &water.id,
            &water.geometry,
            projection,
            &mut nodes,
            &mut fit_points,
            0x5b9bd5,
            Some(0.72),
            0.15,
        );
    }
    for land_use in &scenario.land_use_areas {
        append_area_node(
            &land_use.id,
            &land_use.geometry,
            projection,
            &mut nodes,
            &mut fit_points,
            0x7fa36b,
            None,
            0.08,
        );
    }

    finish_frame(nodes, fit_points, aspect)
}

pub fn build_save_render_frame(
    save: &CitySave,
    aspect: f32,
) -> Result<RendererFrame, CameraError> {
    let projection = GameWorldProjection::from_save(save);
    let mut nodes = Vec::new();
    let mut fit_points = Vec::new();
    let planning = &save.world.planning;

    for road in save.effective_roads() {
        append_road_nodes(
            &road.id,
            &road.geometry,
            road.class,
            projection,
            &mut nodes,
            &mut fit_points,
        );
    }
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
        append_area_node(
            &water.id,
            &water.geometry,
            projection,
            &mut nodes,
            &mut fit_points,
            0x5b9bd5,
            Some(0.72),
            0.15,
        );
    }
    for land_use in &save.scenario.land_use_areas {
        if planning.is_suppressed(&land_use.id) {
            continue;
        }
        append_area_node(
            &land_use.id,
            &land_use.geometry,
            projection,
            &mut nodes,
            &mut fit_points,
            0x7fa36b,
            None,
            0.08,
        );
    }
    for zone in planning.zones.values() {
        append_area_node(
            &zone.id,
            &zone.geometry,
            projection,
            &mut nodes,
            &mut fit_points,
            zone_color(zone.kind),
            Some(0.34),
            0.12,
        );
    }

    finish_frame(nodes, fit_points, aspect)
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
    Ok(RendererFrame {
        camera: RendererCamera {
            aspect,
            view_matrix: camera.view_matrix().elements,
            projection_matrix: camera.projection_matrix().elements,
        },
        nodes,
    })
}

fn fit_orthographic_camera(
    fit_points: &[Vec3],
    aspect: f32,
) -> Result<OrthographicCamera, CameraError> {
    if !aspect.is_finite() || aspect <= 0.0 {
        return Err(CameraError::InvalidAspect);
    }

    let max_coordinate = fit_points.iter().fold(0.0_f32, |extent, point| {
        extent
            .max(point.x.abs())
            .max(point.y.abs())
            .max(point.z.abs())
    });
    let eye_distance = (max_coordinate * 2.0).max(100.0);
    let eye = Vec3::new(eye_distance, eye_distance * 1.35, eye_distance);
    let target = Vec3::ZERO;
    let up = Vec3::new(0.0, 1.0, 0.0);

    if fit_points.is_empty() {
        let half_height = MIN_VERTICAL_SPAN * 0.5;
        let half_width = half_height * aspect;
        return OrthographicCamera::new(
            eye,
            target,
            up,
            -half_width,
            half_width,
            -half_height,
            half_height,
            0.1,
            eye_distance * 8.0,
        );
    }

    let orientation_camera = OrthographicCamera::new(
        eye,
        target,
        up,
        -1.0,
        1.0,
        -1.0,
        1.0,
        0.1,
        eye_distance * 8.0,
    )?;
    let view = orientation_camera.view_matrix();

    let first = view.transform_point(fit_points[0]);
    let mut min_x = first.x;
    let mut max_x = first.x;
    let mut min_y = first.y;
    let mut max_y = first.y;
    let mut min_z = first.z;
    let mut max_z = first.z;

    for point in &fit_points[1..] {
        let camera_point = view.transform_point(*point);
        min_x = min_x.min(camera_point.x);
        max_x = max_x.max(camera_point.x);
        min_y = min_y.min(camera_point.y);
        max_y = max_y.max(camera_point.y);
        min_z = min_z.min(camera_point.z);
        max_z = max_z.max(camera_point.z);
    }

    let center_x = (min_x + max_x) * 0.5;
    let center_y = (min_y + max_y) * 0.5;
    let content_width = (max_x - min_x).max(1.0) * FRAMING_PADDING;
    let content_height = ((max_y - min_y).max(1.0) * FRAMING_PADDING).max(MIN_VERTICAL_SPAN);

    let (frustum_width, frustum_height) = if content_width / content_height > aspect {
        (content_width, content_width / aspect)
    } else {
        (content_height * aspect, content_height)
    };

    let half_width = frustum_width * 0.5;
    let half_height = frustum_height * 0.5;
    let nearest_depth = -max_z;
    let farthest_depth = -min_z;
    let depth_padding = ((farthest_depth - nearest_depth).abs() * 0.1).max(1.0);
    let near = (nearest_depth - depth_padding).max(0.1);
    let far = (farthest_depth + depth_padding).max(near + 1.0);

    OrthographicCamera::new(
        eye,
        target,
        up,
        center_x - half_width,
        center_x + half_width,
        center_y - half_height,
        center_y + half_height,
        near,
        far,
    )
}

fn append_road_nodes(
    id: &str,
    geometry: &Geometry,
    class: RoadClass,
    projection: GameWorldProjection,
    nodes: &mut Vec<RendererSceneNode>,
    fit_points: &mut Vec<Vec3>,
) {
    let width = road_width(class);
    let mut segment_index = 0_usize;
    visit_lines(geometry, &mut |line| {
        for pair in line.windows(2) {
            let [start_x, start_z] = projection.project(pair[0]);
            let [end_x, end_z] = projection.project(pair[1]);
            let dx = end_x - start_x;
            let dz = end_z - start_z;
            let length = dx.hypot(dz);
            if length <= f32::EPSILON {
                continue;
            }
            let angle = dx.atan2(dz);
            let half = angle * 0.5;
            nodes.push(RendererSceneNode {
                id: format!("{id}/road-segment-{segment_index}"),
                geometry: RendererGeometry::Box {
                    size: [width, 0.25, length],
                },
                color: 0x51545a,
                opacity: None,
                transform: RendererTransform {
                    translation: [(start_x + end_x) * 0.5, 0.125, (start_z + end_z) * 0.5],
                    scale: None,
                    rotation_quaternion: Some([0.0, half.sin(), 0.0, half.cos()]),
                },
            });
            append_road_fit_points(fit_points, [start_x, start_z], [end_x, end_z], width, 0.25);
            segment_index += 1;
        }
    });
}

fn append_road_fit_points(
    fit_points: &mut Vec<Vec3>,
    start: [f32; 2],
    end: [f32; 2],
    width: f32,
    height: f32,
) {
    let dx = end[0] - start[0];
    let dz = end[1] - start[1];
    let length = dx.hypot(dz);
    if length <= f32::EPSILON {
        return;
    }
    let half_width = width * 0.5;
    let normal_x = -dz / length * half_width;
    let normal_z = dx / length * half_width;
    for y in [0.0, height] {
        for [x, z] in [
            [start[0] + normal_x, start[1] + normal_z],
            [start[0] - normal_x, start[1] - normal_z],
            [end[0] + normal_x, end[1] + normal_z],
            [end[0] - normal_x, end[1] - normal_z],
        ] {
            fit_points.push(Vec3::new(x, y, z));
        }
    }
}

fn append_building_node(
    building: &ScenarioBuilding,
    projection: GameWorldProjection,
    nodes: &mut Vec<RendererSceneNode>,
    fit_points: &mut Vec<Vec3>,
) {
    append_area_node(
        &building.id,
        &building.footprint,
        projection,
        nodes,
        fit_points,
        0xb5aa98,
        None,
        building_height(building),
    );
}

#[allow(clippy::too_many_arguments)]
fn append_area_node(
    id: &str,
    geometry: &Geometry,
    projection: GameWorldProjection,
    nodes: &mut Vec<RendererSceneNode>,
    fit_points: &mut Vec<Vec3>,
    color: u32,
    opacity: Option<f32>,
    height: f32,
) {
    let Some(bounds) = projected_bounds(geometry, projection) else {
        return;
    };
    let width = (bounds.max_x - bounds.min_x).abs().max(1.0);
    let depth = (bounds.max_z - bounds.min_z).abs().max(1.0);
    let height = height.max(0.05);
    let translation = [
        (bounds.min_x + bounds.max_x) * 0.5,
        height * 0.5,
        (bounds.min_z + bounds.max_z) * 0.5,
    ];
    let size = [width, height, depth];
    nodes.push(RendererSceneNode {
        id: id.to_owned(),
        geometry: RendererGeometry::Box { size },
        color,
        opacity,
        transform: RendererTransform {
            translation,
            scale: None,
            rotation_quaternion: None,
        },
    });
    append_axis_aligned_box_fit_points(fit_points, translation, size);
}

fn append_axis_aligned_box_fit_points(
    fit_points: &mut Vec<Vec3>,
    translation: [f32; 3],
    size: [f32; 3],
) {
    let half = [size[0] * 0.5, size[1] * 0.5, size[2] * 0.5];
    for x_sign in [-1.0, 1.0] {
        for y_sign in [-1.0, 1.0] {
            for z_sign in [-1.0, 1.0] {
                fit_points.push(Vec3::new(
                    translation[0] + x_sign * half[0],
                    translation[1] + y_sign * half[1],
                    translation[2] + z_sign * half[2],
                ));
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ProjectedBounds {
    min_x: f32,
    max_x: f32,
    min_z: f32,
    max_z: f32,
}

fn projected_bounds(
    geometry: &Geometry,
    projection: GameWorldProjection,
) -> Option<ProjectedBounds> {
    let mut positions = Vec::new();
    collect_positions(geometry, &mut positions);
    let mut projected = positions
        .into_iter()
        .map(|position| projection.project(position));
    let [first_x, first_z] = projected.next()?;
    let mut bounds = ProjectedBounds {
        min_x: first_x,
        max_x: first_x,
        min_z: first_z,
        max_z: first_z,
    };
    for [x, z] in projected {
        bounds.min_x = bounds.min_x.min(x);
        bounds.max_x = bounds.max_x.max(x);
        bounds.min_z = bounds.min_z.min(z);
        bounds.max_z = bounds.max_z.max(z);
    }
    Some(bounds)
}

fn road_width(class: RoadClass) -> f32 {
    match class {
        RoadClass::Motorway | RoadClass::Trunk => 12.0,
        RoadClass::Primary | RoadClass::Secondary => 9.0,
        RoadClass::Tertiary => 7.0,
        RoadClass::Residential | RoadClass::Service | RoadClass::Other => 5.5,
        RoadClass::Track => 4.5,
        RoadClass::Pedestrian | RoadClass::Cycleway | RoadClass::Footway => 3.0,
    }
}

fn building_height(building: &ScenarioBuilding) -> f32 {
    building
        .height_m
        .or_else(|| building.levels.map(|levels| f32::from(levels) * 3.0))
        .unwrap_or(9.0)
        .clamp(2.5, 400.0)
}

fn zone_color(kind: ZoneKind) -> u32 {
    match kind {
        ZoneKind::Residential => 0x79a86b,
        ZoneKind::Commercial => 0x6a8eb5,
        ZoneKind::Industrial => 0xb59a62,
        ZoneKind::MixedUse => 0x8d76ad,
    }
}

fn collect_scenario_positions(scenario: &CityScenario, output: &mut Vec<[f64; 2]>) {
    for road in &scenario.roads {
        collect_positions(&road.geometry, output);
    }
    for building in &scenario.buildings {
        collect_positions(&building.footprint, output);
    }
    for water in &scenario.water {
        collect_positions(&water.geometry, output);
    }
    for land_use in &scenario.land_use_areas {
        collect_positions(&land_use.geometry, output);
    }
    for transit in &scenario.transit_anchors {
        collect_positions(&transit.geometry, output);
    }
}

fn visit_lines(geometry: &Geometry, visitor: &mut impl FnMut(&[[f64; 2]])) {
    match geometry {
        Geometry::LineString { coordinates } => visitor(coordinates),
        Geometry::MultiLineString { coordinates } => {
            for line in coordinates {
                visitor(line);
            }
        }
        Geometry::Polygon { coordinates } => {
            for line in coordinates {
                visitor(line);
            }
        }
        Geometry::MultiPolygon { coordinates } => {
            for polygon in coordinates {
                for line in polygon {
                    visitor(line);
                }
            }
        }
        Geometry::GeometryCollection { geometries } => {
            for geometry in geometries {
                visit_lines(geometry, visitor);
            }
        }
        Geometry::Point { .. } | Geometry::MultiPoint { .. } => {}
    }
}

fn collect_positions(geometry: &Geometry, output: &mut Vec<[f64; 2]>) {
    match geometry {
        Geometry::Point { coordinates } => output.push(*coordinates),
        Geometry::MultiPoint { coordinates } | Geometry::LineString { coordinates } => {
            output.extend(coordinates.iter().copied());
        }
        Geometry::MultiLineString { coordinates } | Geometry::Polygon { coordinates } => {
            for line in coordinates {
                output.extend(line.iter().copied());
            }
        }
        Geometry::MultiPolygon { coordinates } => {
            for polygon in coordinates {
                for line in polygon {
                    output.extend(line.iter().copied());
                }
            }
        }
        Geometry::GeometryCollection { geometries } => {
            for geometry in geometries {
                collect_positions(geometry, output);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        BuildingUse, ExternalRevision, PlannedRoad, PlannedZone, PlanningCommand,
        SCENARIO_SCHEMA_VERSION, ScenarioBuilding, ScenarioProvenance, ScenarioRoad, ZoneKind,
    };

    use super::*;

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
    fn save_frame_projects_effective_planning_overlay() {
        let mut save = CitySave::new(scenario());
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
    fn fitted_camera_encloses_rotated_wide_scene_and_preserves_aspect() {
        let fit_points = vec![
            Vec3::new(-500.0, 0.0, -500.0),
            Vec3::new(-500.0, 0.0, 500.0),
            Vec3::new(500.0, 0.0, -500.0),
            Vec3::new(500.0, 0.0, 500.0),
            Vec3::new(-500.0, 240.0, -500.0),
            Vec3::new(-500.0, 240.0, 500.0),
            Vec3::new(500.0, 240.0, -500.0),
            Vec3::new(500.0, 240.0, 500.0),
        ];
        let aspect = 0.75;
        let camera = fit_orthographic_camera(&fit_points, aspect).unwrap();
        let view = camera.view_matrix();

        for point in fit_points {
            let camera_point = view.transform_point(point);
            let depth = -camera_point.z;
            assert!(camera_point.x >= camera.left && camera_point.x <= camera.right);
            assert!(camera_point.y >= camera.bottom && camera_point.y <= camera.top);
            assert!(depth >= camera.near && depth <= camera.far);
        }

        let fitted_aspect = (camera.right - camera.left) / (camera.top - camera.bottom);
        assert!((fitted_aspect - aspect).abs() <= 1.0e-5);
    }

    #[test]
    fn invalid_viewport_aspect_fails_closed() {
        assert_eq!(
            build_render_frame(&scenario(), 0.0),
            Err(CameraError::InvalidAspect)
        );
    }
}
