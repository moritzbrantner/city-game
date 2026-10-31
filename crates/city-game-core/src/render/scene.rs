use core::fmt::Write as _;

use geo_core::Geometry;
use three_d_core::Vec3;

use super::frame::{RendererGeometry, RendererSceneNode, RendererTransform};
use super::projection::{GameWorldProjection, projected_bounds, visit_lines};
use crate::{PlannedZone, RoadClass, ScenarioBuilding, ScenarioLandUse, ScenarioWater, ZoneKind};

pub(super) fn append_road_nodes(
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
                id: road_segment_node_id(id, segment_index),
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
    fit_points.reserve(8);
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

pub(super) fn append_building_node(
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

pub(super) fn road_segment_node_id(road_id: &str, segment_index: usize) -> String {
    // Sized exactly: this runs once per road segment per rebuilt frame.
    const SEPARATOR: &str = "/road-segment-";
    let digits = segment_index
        .checked_ilog10()
        .map_or(1, |log| log as usize + 1);
    let mut id = String::with_capacity(road_id.len() + SEPARATOR.len() + digits);
    id.push_str(road_id);
    id.push_str(SEPARATOR);
    write!(id, "{segment_index}").expect("writing to a String cannot fail");
    id
}

pub(super) fn append_water_node(
    water: &ScenarioWater,
    projection: GameWorldProjection,
    nodes: &mut Vec<RendererSceneNode>,
    fit_points: &mut Vec<Vec3>,
) {
    append_area_node(
        &water.id,
        &water.geometry,
        projection,
        nodes,
        fit_points,
        0x5b9bd5,
        Some(0.72),
        0.15,
    );
}

pub(super) fn append_land_use_node(
    land_use: &ScenarioLandUse,
    projection: GameWorldProjection,
    nodes: &mut Vec<RendererSceneNode>,
    fit_points: &mut Vec<Vec3>,
) {
    append_area_node(
        &land_use.id,
        &land_use.geometry,
        projection,
        nodes,
        fit_points,
        0x7fa36b,
        None,
        0.08,
    );
}

pub(super) fn append_zone_node(
    zone: &PlannedZone,
    projection: GameWorldProjection,
    nodes: &mut Vec<RendererSceneNode>,
    fit_points: &mut Vec<Vec3>,
) {
    append_area_node(
        &zone.id,
        &zone.geometry,
        projection,
        nodes,
        fit_points,
        zone_color(zone.kind),
        Some(0.34),
        0.12,
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
    fit_points.reserve(8);
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
