use geo_core::Geometry;

use crate::{CitySave, CityScenario};

#[derive(Debug, Clone, Copy)]
pub(super) struct GameWorldProjection {
    origin_lon: f64,
    origin_lat: f64,
    longitude_meters_per_degree: f64,
    latitude_meters_per_degree: f64,
}

impl GameWorldProjection {
    pub(super) fn from_scenario(scenario: &CityScenario) -> Self {
        let mut bounds = None;
        visit_scenario_positions(scenario, &mut |position| {
            extend_geographic_bounds(&mut bounds, position);
        });
        Self::from_bounds(bounds)
    }

    pub(super) fn from_save(save: &CitySave) -> Self {
        let mut bounds = None;
        visit_scenario_positions(&save.scenario, &mut |position| {
            extend_geographic_bounds(&mut bounds, position);
        });
        for road in save.world.planning.player_roads.values() {
            visit_positions(&road.geometry, &mut |position| {
                extend_geographic_bounds(&mut bounds, position);
            });
        }
        for zone in save.world.planning.zones.values() {
            visit_positions(&zone.geometry, &mut |position| {
                extend_geographic_bounds(&mut bounds, position);
            });
        }
        Self::from_bounds(bounds)
    }

    pub(super) fn from_bounds(bounds: Option<GeographicBounds>) -> Self {
        let [origin_lon, origin_lat] = bounds.map(GeographicBounds::center).unwrap_or([0.0, 0.0]);
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

    /// The projection is fully determined by its origin.
    pub(super) fn same_origin(self, other: Self) -> bool {
        self.origin_lon.to_bits() == other.origin_lon.to_bits()
            && self.origin_lat.to_bits() == other.origin_lat.to_bits()
    }

    pub(super) fn project(self, position: [f64; 2]) -> [f32; 2] {
        let x = (position[0] - self.origin_lon) * self.longitude_meters_per_degree;
        let z = -(position[1] - self.origin_lat) * self.latitude_meters_per_degree;
        [x as f32, z as f32]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct GeographicBounds {
    min_lon: f64,
    max_lon: f64,
    min_lat: f64,
    max_lat: f64,
}

impl GeographicBounds {
    fn from_position([lon, lat]: [f64; 2]) -> Self {
        Self {
            min_lon: lon,
            max_lon: lon,
            min_lat: lat,
            max_lat: lat,
        }
    }

    fn include(&mut self, [lon, lat]: [f64; 2]) {
        self.min_lon = self.min_lon.min(lon);
        self.max_lon = self.max_lon.max(lon);
        self.min_lat = self.min_lat.min(lat);
        self.max_lat = self.max_lat.max(lat);
    }

    pub(super) fn merge(self, other: Self) -> Self {
        Self {
            min_lon: self.min_lon.min(other.min_lon),
            max_lon: self.max_lon.max(other.max_lon),
            min_lat: self.min_lat.min(other.min_lat),
            max_lat: self.max_lat.max(other.max_lat),
        }
    }

    /// Whether removing `self` from a set whose merged bounds are `aggregate` can change them.
    pub(super) fn touches(self, aggregate: Self) -> bool {
        self.min_lon == aggregate.min_lon
            || self.max_lon == aggregate.max_lon
            || self.min_lat == aggregate.min_lat
            || self.max_lat == aggregate.max_lat
    }

    fn center(self) -> [f64; 2] {
        [
            (self.min_lon + self.max_lon) * 0.5,
            (self.min_lat + self.max_lat) * 0.5,
        ]
    }
}

pub(super) fn extend_geographic_bounds(bounds: &mut Option<GeographicBounds>, position: [f64; 2]) {
    match bounds {
        Some(bounds) => bounds.include(position),
        None => *bounds = Some(GeographicBounds::from_position(position)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ProjectedBounds {
    pub(super) min_x: f32,
    pub(super) max_x: f32,
    pub(super) min_z: f32,
    pub(super) max_z: f32,
}

impl ProjectedBounds {
    fn from_position([x, z]: [f32; 2]) -> Self {
        Self {
            min_x: x,
            max_x: x,
            min_z: z,
            max_z: z,
        }
    }

    fn include(&mut self, [x, z]: [f32; 2]) {
        self.min_x = self.min_x.min(x);
        self.max_x = self.max_x.max(x);
        self.min_z = self.min_z.min(z);
        self.max_z = self.max_z.max(z);
    }
}

pub(super) fn projected_bounds(
    geometry: &Geometry,
    projection: GameWorldProjection,
) -> Option<ProjectedBounds> {
    let mut bounds: Option<ProjectedBounds> = None;
    visit_positions(geometry, &mut |position| {
        let projected = projection.project(position);
        match &mut bounds {
            Some(bounds) => bounds.include(projected),
            None => bounds = Some(ProjectedBounds::from_position(projected)),
        }
    });
    bounds
}

pub(super) fn visit_scenario_positions(
    scenario: &CityScenario,
    visitor: &mut impl FnMut([f64; 2]),
) {
    for road in &scenario.roads {
        visit_positions(&road.geometry, visitor);
    }
    for building in &scenario.buildings {
        visit_positions(&building.footprint, visitor);
    }
    for water in &scenario.water {
        visit_positions(&water.geometry, visitor);
    }
    for land_use in &scenario.land_use_areas {
        visit_positions(&land_use.geometry, visitor);
    }
    for transit in &scenario.transit_anchors {
        visit_positions(&transit.geometry, visitor);
    }
}

pub(super) fn visit_lines(geometry: &Geometry, visitor: &mut impl FnMut(&[[f64; 2]])) {
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

pub(super) fn visit_positions(geometry: &Geometry, visitor: &mut impl FnMut([f64; 2])) {
    match geometry {
        Geometry::Point { coordinates } => visitor(*coordinates),
        Geometry::MultiPoint { coordinates } | Geometry::LineString { coordinates } => {
            for &position in coordinates {
                visitor(position);
            }
        }
        Geometry::MultiLineString { coordinates } | Geometry::Polygon { coordinates } => {
            for line in coordinates {
                for &position in line {
                    visitor(position);
                }
            }
        }
        Geometry::MultiPolygon { coordinates } => {
            for polygon in coordinates {
                for line in polygon {
                    for &position in line {
                        visitor(position);
                    }
                }
            }
        }
        Geometry::GeometryCollection { geometries } => {
            for geometry in geometries {
                visit_positions(geometry, visitor);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_collect_positions(geometry: &Geometry, output: &mut Vec<[f64; 2]>) {
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
                    legacy_collect_positions(geometry, output);
                }
            }
        }
    }

    fn legacy_projection_from_positions(positions: &[[f64; 2]]) -> GameWorldProjection {
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
        GameWorldProjection {
            origin_lon,
            origin_lat,
            longitude_meters_per_degree: latitude_meters_per_degree * origin_lat.to_radians().cos(),
            latitude_meters_per_degree,
        }
    }

    fn legacy_projected_bounds(
        geometry: &Geometry,
        projection: GameWorldProjection,
    ) -> Option<ProjectedBounds> {
        let mut positions = Vec::new();
        legacy_collect_positions(geometry, &mut positions);
        let mut projected = positions
            .into_iter()
            .map(|position| projection.project(position));
        let [first_x, first_z] = projected.next()?;
        let mut bounds = ProjectedBounds::from_position([first_x, first_z]);
        for projected in projected {
            bounds.include(projected);
        }
        Some(bounds)
    }

    #[test]
    fn streamed_coordinate_bounds_match_materialized_reference_exactly() {
        let geometry = Geometry::GeometryCollection {
            geometries: vec![
                Geometry::Point {
                    coordinates: [7.999, 48.002],
                },
                Geometry::MultiPoint {
                    coordinates: vec![[8.004, 47.998], [8.006, 48.005]],
                },
                Geometry::MultiLineString {
                    coordinates: vec![
                        vec![[8.0, 48.0], [8.001, 48.001]],
                        vec![[8.002, 47.999], [8.003, 48.004]],
                    ],
                },
                Geometry::MultiPolygon {
                    coordinates: vec![vec![vec![
                        [8.01, 48.01],
                        [8.012, 48.01],
                        [8.012, 48.012],
                        [8.01, 48.01],
                    ]]],
                },
            ],
        };

        let mut streamed_positions = Vec::new();
        visit_positions(&geometry, &mut |position| streamed_positions.push(position));
        let mut materialized_positions = Vec::new();
        legacy_collect_positions(&geometry, &mut materialized_positions);
        assert_eq!(streamed_positions, materialized_positions);

        let reference_projection = legacy_projection_from_positions(&materialized_positions);
        let mut geographic_bounds = None;
        visit_positions(&geometry, &mut |position| {
            extend_geographic_bounds(&mut geographic_bounds, position);
        });
        let streamed_projection = GameWorldProjection::from_bounds(geographic_bounds);

        assert_eq!(
            streamed_projection.origin_lon,
            reference_projection.origin_lon
        );
        assert_eq!(
            streamed_projection.origin_lat,
            reference_projection.origin_lat
        );
        assert_eq!(
            streamed_projection.longitude_meters_per_degree,
            reference_projection.longitude_meters_per_degree
        );
        assert_eq!(
            streamed_projection.latitude_meters_per_degree,
            reference_projection.latitude_meters_per_degree
        );
        assert_eq!(
            projected_bounds(&geometry, streamed_projection),
            legacy_projected_bounds(&geometry, reference_projection)
        );
    }

    #[test]
    fn streamed_projection_preserves_empty_geometry_defaults() {
        let geometry = Geometry::GeometryCollection {
            geometries: Vec::new(),
        };
        let projection = GameWorldProjection::from_bounds(None);

        assert_eq!(projection.origin_lon, 0.0);
        assert_eq!(projection.origin_lat, 0.0);
        assert_eq!(projected_bounds(&geometry, projection), None);
    }
}
