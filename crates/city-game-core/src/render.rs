use geo_core::Geometry;
use serde::{Deserialize, Serialize};
use three_d_camera::{CameraError, OrthographicCamera};
use three_d_core::Vec3;

use crate::{CityScenario, ScenarioFeature, ScenarioFeatureKind};

pub const THREE_D_LAB_REVISION: &str = "6a18cb2d1fe9efdbae619c144b2180fdeb472172";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RendererCamera {
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
        for feature in &scenario.features {
            collect_positions(&feature.geometry, &mut positions);
        }
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
    let mut max_extent = 0.0_f32;

    for feature in &scenario.features {
        update_extent(feature, projection, &mut max_extent);
        match feature.kind {
            ScenarioFeatureKind::Road => append_road_nodes(feature, projection, &mut nodes),
            ScenarioFeatureKind::Building => append_area_node(
                feature,
                projection,
                &mut nodes,
                0xb5aa98,
                building_height(feature),
            ),
            ScenarioFeatureKind::Water => {
                append_area_node(feature, projection, &mut nodes, 0x5b9bd5, 0.15)
            }
            ScenarioFeatureKind::LandUse => {
                append_area_node(feature, projection, &mut nodes, 0x7fa36b, 0.08)
            }
            ScenarioFeatureKind::Transit | ScenarioFeatureKind::Other => {}
        }
    }
    nodes.sort_by(|left, right| left.id.cmp(&right.id));

    let half_height = (max_extent * 0.85).max(75.0);
    let half_width = half_height * aspect.max(0.25);
    let eye_distance = half_height.max(100.0);
    let camera = OrthographicCamera::new(
        Vec3::new(eye_distance, eye_distance * 1.35, eye_distance),
        Vec3::ZERO,
        Vec3::new(0.0, 1.0, 0.0),
        -half_width,
        half_width,
        -half_height,
        half_height,
        0.1,
        eye_distance * 8.0,
    )?;

    Ok(RendererFrame {
        camera: RendererCamera {
            view_matrix: camera.view_matrix().elements,
            projection_matrix: camera.projection_matrix().elements,
        },
        nodes,
    })
}

fn update_extent(feature: &ScenarioFeature, projection: GameWorldProjection, max_extent: &mut f32) {
    let mut positions = Vec::new();
    collect_positions(&feature.geometry, &mut positions);
    for position in positions {
        let [x, z] = projection.project(position);
        *max_extent = (*max_extent).max(x.abs()).max(z.abs());
    }
}

fn append_road_nodes(
    feature: &ScenarioFeature,
    projection: GameWorldProjection,
    nodes: &mut Vec<RendererSceneNode>,
) {
    let width = road_width(feature);
    let mut segment_index = 0_usize;
    visit_lines(&feature.geometry, &mut |line| {
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
                id: format!("{}/road-segment-{segment_index}", feature.source_id),
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
            segment_index += 1;
        }
    });
}

fn append_area_node(
    feature: &ScenarioFeature,
    projection: GameWorldProjection,
    nodes: &mut Vec<RendererSceneNode>,
    color: u32,
    height: f32,
) {
    let Some(bounds) = projected_bounds(&feature.geometry, projection) else {
        return;
    };
    let width = (bounds.max_x - bounds.min_x).abs().max(1.0);
    let depth = (bounds.max_z - bounds.min_z).abs().max(1.0);
    nodes.push(RendererSceneNode {
        id: feature.source_id.clone(),
        geometry: RendererGeometry::Box {
            size: [width, height.max(0.05), depth],
        },
        color,
        opacity: (feature.kind == ScenarioFeatureKind::Water).then_some(0.72),
        transform: RendererTransform {
            translation: [
                (bounds.min_x + bounds.max_x) * 0.5,
                height.max(0.05) * 0.5,
                (bounds.min_z + bounds.max_z) * 0.5,
            ],
            scale: None,
            rotation_quaternion: None,
        },
    });
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

fn road_width(feature: &ScenarioFeature) -> f32 {
    match feature.tags.get("highway").map(String::as_str) {
        Some("motorway" | "trunk") => 12.0,
        Some("primary" | "secondary") => 9.0,
        Some("tertiary") => 7.0,
        _ => 5.5,
    }
}

fn building_height(feature: &ScenarioFeature) -> f32 {
    if let Some(height) = feature
        .tags
        .get("height")
        .and_then(|value| value.trim_end_matches('m').trim().parse::<f32>().ok())
    {
        return height.clamp(2.5, 400.0);
    }
    if let Some(levels) = feature
        .tags
        .get("building:levels")
        .and_then(|value| value.parse::<f32>().ok())
    {
        return (levels * 3.0).clamp(2.5, 400.0);
    }
    9.0
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
    use std::collections::BTreeMap;

    use crate::{ExternalRevision, SCENARIO_SCHEMA_VERSION, ScenarioProvenance};

    use super::*;

    #[test]
    fn frame_uses_shared_camera_contract_and_stable_node_order() {
        let scenario = CityScenario {
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
            features: vec![
                ScenarioFeature {
                    source_id: "way/20".to_owned(),
                    kind: ScenarioFeatureKind::Building,
                    tags: BTreeMap::from([("building".to_owned(), "yes".to_owned())]),
                    geometry: Geometry::Polygon {
                        coordinates: vec![vec![
                            [8.0, 48.0],
                            [8.001, 48.0],
                            [8.001, 48.001],
                            [8.0, 48.0],
                        ]],
                    },
                },
                ScenarioFeature {
                    source_id: "way/10".to_owned(),
                    kind: ScenarioFeatureKind::Road,
                    tags: BTreeMap::from([("highway".to_owned(), "residential".to_owned())]),
                    geometry: Geometry::LineString {
                        coordinates: vec![[8.0, 48.0], [8.001, 48.001]],
                    },
                },
            ],
        };

        let first = build_render_frame(&scenario, 16.0 / 9.0).unwrap();
        let second = build_render_frame(&scenario, 16.0 / 9.0).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.nodes[0].id, "way/10/road-segment-0");
        assert_eq!(first.nodes[1].id, "way/20");

        let json = serde_json::to_value(first).unwrap();
        assert!(json["camera"]["viewMatrix"].is_array());
        assert!(json["camera"]["projectionMatrix"].is_array());
        assert_eq!(json["nodes"][0]["geometry"]["kind"], "box");
    }
}
