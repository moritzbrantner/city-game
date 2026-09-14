use std::fmt::Write as _;

use geo_core::Geometry;
use geo_io_osm::{
    CollectOsmBytesOptions, IndexOptions, OsmElementType, OsmFeature, OsmFilterSpec, OsmTags,
    collect_osm_pbf_bytes,
};
use sha2::{Digest, Sha256};

use crate::{
    BuildingUse, CityScenario, ExternalRevision, LandUseKind, RoadClass, SCENARIO_SCHEMA_VERSION,
    ScenarioBuilding, ScenarioLandUse, ScenarioProvenance, ScenarioRoad, ScenarioTransitAnchor,
    ScenarioWater, TransitKind, WaterKind,
};

pub const GEO_ANALYSIS_REVISION: &str = "c4df63a023f2183d700a9a28071d732d345e7c25";
pub const OSM_PARSER_REPOSITORY: &str = "https://github.com/moritzbrantner/geo-analysis";

const LATITUDE_METERS_PER_DEGREE: f64 = 111_320.0;

pub fn import_osm_pbf_bytes(
    source_name: impl Into<String>,
    input: &[u8],
) -> geo_core::Result<CityScenario> {
    let mut spec = OsmFilterSpec::default();
    spec.filter.types = Some(vec![
        OsmElementType::Node,
        OsmElementType::Way,
        OsmElementType::Relation,
    ]);

    let collected = collect_osm_pbf_bytes(CollectOsmBytesOptions {
        input,
        spec,
        index_options: IndexOptions::default(),
    })?;

    let mut roads = Vec::new();
    let mut buildings = Vec::new();
    let mut water = Vec::new();
    let mut land_use_areas = Vec::new();
    let mut transit_anchors = Vec::new();

    for feature in collected.features {
        match normalize_feature(feature) {
            NormalizedFeature::Road(value) => roads.push(value),
            NormalizedFeature::Building(value) => buildings.push(value),
            NormalizedFeature::Water(value) => water.push(value),
            NormalizedFeature::LandUse(value) => land_use_areas.push(value),
            NormalizedFeature::Transit(value) => transit_anchors.push(value),
            NormalizedFeature::Ignore => {}
        }
    }

    roads.sort_by(|left, right| left.id.cmp(&right.id));
    buildings.sort_by(|left, right| left.id.cmp(&right.id));
    water.sort_by(|left, right| left.id.cmp(&right.id));
    land_use_areas.sort_by(|left, right| left.id.cmp(&right.id));
    transit_anchors.sort_by(|left, right| left.id.cmp(&right.id));

    Ok(CityScenario {
        schema_version: SCENARIO_SCHEMA_VERSION,
        provenance: ScenarioProvenance {
            source_format: "osm-pbf".to_owned(),
            source_name: source_name.into(),
            source_sha256: sha256_hex(input),
            parser: ExternalRevision {
                repository: OSM_PARSER_REPOSITORY.to_owned(),
                revision: GEO_ANALYSIS_REVISION.to_owned(),
            },
        },
        roads,
        buildings,
        water,
        land_use_areas,
        transit_anchors,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportedKind {
    Road,
    Building,
    Water,
    LandUse,
    Transit,
    Ignore,
}

enum NormalizedFeature {
    Road(ScenarioRoad),
    Building(ScenarioBuilding),
    Water(ScenarioWater),
    LandUse(ScenarioLandUse),
    Transit(ScenarioTransitAnchor),
    Ignore,
}

fn normalize_feature(feature: OsmFeature) -> NormalizedFeature {
    let source_id = feature.stable_id();
    let id = format!("imported/{source_id}");
    let kind = classify(&feature.tags);
    let name = feature.tags.get("name").cloned();

    match kind {
        ImportedKind::Road => NormalizedFeature::Road(ScenarioRoad {
            id,
            source_id,
            class: road_class(&feature.tags),
            name,
            lanes: feature
                .tags
                .get("lanes")
                .and_then(|value| parse_lanes(value)),
            max_speed_kph: feature
                .tags
                .get("maxspeed")
                .and_then(|value| parse_max_speed_kph(value)),
            geometry: feature.geometry,
        }),
        ImportedKind::Building => {
            let use_kind = building_use(&feature.tags);
            let levels = feature
                .tags
                .get("building:levels")
                .and_then(|value| parse_positive_u16(value));
            let height_m = feature
                .tags
                .get("height")
                .and_then(|value| parse_height_m(value));
            let footprint = canonical_building_footprint(feature.geometry);
            let gross_floor_area_m2 = gross_floor_area_m2(&footprint, levels, height_m);
            NormalizedFeature::Building(ScenarioBuilding {
                id,
                source_id,
                use_kind,
                name,
                levels,
                height_m,
                gross_floor_area_m2,
                footprint,
            })
        }
        ImportedKind::Water => NormalizedFeature::Water(ScenarioWater {
            id,
            source_id,
            kind: water_kind(&feature.tags),
            geometry: feature.geometry,
        }),
        ImportedKind::LandUse => NormalizedFeature::LandUse(ScenarioLandUse {
            id,
            source_id,
            kind: land_use_kind(&feature.tags),
            geometry: feature.geometry,
        }),
        ImportedKind::Transit => NormalizedFeature::Transit(ScenarioTransitAnchor {
            id,
            source_id,
            kind: transit_kind(&feature.tags),
            name,
            geometry: feature.geometry,
        }),
        ImportedKind::Ignore => NormalizedFeature::Ignore,
    }
}

fn classify(tags: &OsmTags) -> ImportedKind {
    if is_transit_feature(tags) {
        ImportedKind::Transit
    } else if tags
        .get("highway")
        .is_some_and(|value| is_road_highway(value))
    {
        ImportedKind::Road
    } else if tags.contains_key("building") {
        ImportedKind::Building
    } else if tags.get("natural").is_some_and(|value| value == "water")
        || tags.contains_key("water")
        || tags.contains_key("waterway")
    {
        ImportedKind::Water
    } else if tags.contains_key("landuse") {
        ImportedKind::LandUse
    } else {
        ImportedKind::Ignore
    }
}

fn is_transit_feature(tags: &OsmTags) -> bool {
    tags.contains_key("railway")
        || tags.contains_key("public_transport")
        || matches!(
            tags.get("highway").map(String::as_str),
            Some("bus_stop" | "platform")
        )
}

fn is_road_highway(value: &str) -> bool {
    !matches!(
        value,
        "bus_stop"
            | "platform"
            | "traffic_signals"
            | "crossing"
            | "stop"
            | "give_way"
            | "street_lamp"
            | "speed_camera"
            | "motorway_junction"
            | "services"
            | "rest_area"
            | "turning_circle"
            | "turning_loop"
            | "passing_place"
            | "milestone"
            | "emergency_bay"
            | "elevator"
    )
}

fn road_class(tags: &OsmTags) -> RoadClass {
    match tags.get("highway").map(String::as_str) {
        Some("motorway" | "motorway_link") => RoadClass::Motorway,
        Some("trunk" | "trunk_link") => RoadClass::Trunk,
        Some("primary" | "primary_link") => RoadClass::Primary,
        Some("secondary" | "secondary_link") => RoadClass::Secondary,
        Some("tertiary" | "tertiary_link") => RoadClass::Tertiary,
        Some("residential" | "living_street" | "unclassified") => RoadClass::Residential,
        Some("service") => RoadClass::Service,
        Some("track") => RoadClass::Track,
        Some("pedestrian") => RoadClass::Pedestrian,
        Some("cycleway") => RoadClass::Cycleway,
        Some("footway" | "path" | "steps") => RoadClass::Footway,
        _ => RoadClass::Other,
    }
}

fn building_use(tags: &OsmTags) -> BuildingUse {
    match tags.get("building").map(String::as_str) {
        Some(
            "apartments" | "house" | "residential" | "detached" | "semidetached_house" | "terrace"
            | "dormitory",
        ) => BuildingUse::Residential,
        Some("commercial" | "retail" | "office" | "hotel") => BuildingUse::Commercial,
        Some("industrial" | "warehouse" | "manufacture") => BuildingUse::Industrial,
        Some(
            "school" | "hospital" | "civic" | "government" | "public" | "church" | "cathedral"
            | "chapel",
        ) => BuildingUse::Civic,
        Some("farm" | "farm_auxiliary" | "barn" | "stable" | "greenhouse") => {
            BuildingUse::Agricultural
        }
        _ => BuildingUse::Other,
    }
}

fn water_kind(tags: &OsmTags) -> WaterKind {
    match tags.get("waterway").map(String::as_str) {
        Some("river") => WaterKind::River,
        Some("stream") => WaterKind::Stream,
        Some("canal") => WaterKind::Canal,
        Some(_) => WaterKind::Other,
        None if tags.get("natural").is_some_and(|value| value == "water")
            || tags.contains_key("water") =>
        {
            WaterKind::Body
        }
        None => WaterKind::Other,
    }
}

fn land_use_kind(tags: &OsmTags) -> LandUseKind {
    match tags.get("landuse").map(String::as_str) {
        Some("residential") => LandUseKind::Residential,
        Some("commercial") => LandUseKind::Commercial,
        Some("industrial") => LandUseKind::Industrial,
        Some("retail") => LandUseKind::Retail,
        Some("forest") => LandUseKind::Forest,
        Some("farmland" | "farmyard" | "orchard" | "vineyard") => LandUseKind::Farmland,
        Some("recreation_ground" | "village_green" | "grass") => LandUseKind::Recreation,
        Some("cemetery") => LandUseKind::Cemetery,
        _ => LandUseKind::Other,
    }
}

fn transit_kind(tags: &OsmTags) -> TransitKind {
    match (
        tags.get("highway").map(String::as_str),
        tags.get("public_transport").map(String::as_str),
        tags.get("railway").map(String::as_str),
    ) {
        (Some("bus_stop"), _, _) => TransitKind::BusStop,
        (_, Some("platform"), _) | (Some("platform"), _, _) => TransitKind::Platform,
        (_, _, Some("station" | "halt")) => TransitKind::Station,
        (_, _, Some("tram_stop")) => TransitKind::TramStop,
        (_, Some("stop_position"), _) => TransitKind::StopPosition,
        (_, _, Some(_)) => TransitKind::Rail,
        _ => TransitKind::Other,
    }
}

fn parse_lanes(value: &str) -> Option<u8> {
    value
        .split([';', '|'])
        .next()?
        .trim()
        .parse::<u8>()
        .ok()
        .filter(|lanes| *lanes > 0 && *lanes <= 32)
}

fn parse_max_speed_kph(value: &str) -> Option<u16> {
    let normalized = value.trim().strip_suffix(" km/h").unwrap_or(value.trim());
    if normalized.ends_with("mph") {
        return None;
    }
    normalized
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|speed| *speed > 0 && *speed <= 300)
}

fn parse_positive_u16(value: &str) -> Option<u16> {
    value.trim().parse::<u16>().ok().filter(|value| *value > 0)
}

fn parse_height_m(value: &str) -> Option<f32> {
    value
        .trim_end_matches('m')
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|height| height.is_finite() && *height > 0.0)
        .map(|height| height.clamp(0.5, 1_000.0))
}

fn canonical_building_footprint(geometry: Geometry) -> Geometry {
    match geometry {
        Geometry::LineString { coordinates }
            if coordinates.len() >= 4 && coordinates.first() == coordinates.last() =>
        {
            Geometry::Polygon {
                coordinates: vec![coordinates],
            }
        }
        other => other,
    }
}

fn gross_floor_area_m2(geometry: &Geometry, levels: Option<u16>, height_m: Option<f32>) -> u64 {
    let footprint_area = geometry_area_m2(geometry);
    let levels = levels.unwrap_or_else(|| {
        height_m
            .map(|height| ((height / 3.0).round() as u16).max(1))
            .unwrap_or(1)
    });
    footprint_area.saturating_mul(u64::from(levels))
}

fn geometry_area_m2(geometry: &Geometry) -> u64 {
    let area = match geometry {
        Geometry::Polygon { coordinates } => polygon_area_m2(coordinates),
        Geometry::MultiPolygon { coordinates } => coordinates
            .iter()
            .map(|polygon| polygon_area_m2(polygon))
            .sum(),
        Geometry::GeometryCollection { geometries } => geometries
            .iter()
            .map(geometry_area_m2)
            .fold(0_u64, u64::saturating_add)
            as f64,
        Geometry::Point { .. }
        | Geometry::MultiPoint { .. }
        | Geometry::LineString { .. }
        | Geometry::MultiLineString { .. } => 0.0,
    };
    if area.is_finite() && area > 0.0 {
        area.round().min(u64::MAX as f64) as u64
    } else {
        0
    }
}

fn polygon_area_m2(rings: &[Vec<[f64; 2]>]) -> f64 {
    let Some(outer) = rings.first() else {
        return 0.0;
    };
    let outer_area = ring_area_m2(outer).abs();
    let holes = rings
        .iter()
        .skip(1)
        .map(|ring| ring_area_m2(ring).abs())
        .sum::<f64>();
    (outer_area - holes).max(0.0)
}

fn ring_area_m2(ring: &[[f64; 2]]) -> f64 {
    if ring.len() < 3 {
        return 0.0;
    }
    let origin_lon = ring[0][0];
    let origin_lat = ring.iter().map(|position| position[1]).sum::<f64>() / ring.len() as f64;
    let longitude_meters_per_degree = LATITUDE_METERS_PER_DEGREE * origin_lat.to_radians().cos();
    let project = |position: [f64; 2]| {
        let longitude_delta = (position[0] - origin_lon + 180.0).rem_euclid(360.0) - 180.0;
        [
            longitude_delta * longitude_meters_per_degree,
            (position[1] - origin_lat) * LATITUDE_METERS_PER_DEGREE,
        ]
    };
    let mut twice_area = 0.0;
    for index in 0..ring.len() {
        let current = project(ring[index]);
        let next = project(ring[(index + 1) % ring.len()]);
        twice_area += current[0] * next[1] - next[0] * current[1];
    }
    twice_area * 0.5
}

fn sha256_hex(input: &[u8]) -> String {
    let digest = Sha256::digest(input);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use osmpbfreader::{fileformat, osmformat};
    use protobuf::Message;

    use super::*;

    fn synthetic_pbf_bytes() -> Vec<u8> {
        let mut string_table = osmformat::StringTable::new();
        for value in [
            "",
            "highway",
            "residential",
            "name",
            "Synthetic Road",
            "building",
            "apartments",
            "Synthetic Building",
            "building:levels",
            "4",
            "lanes",
            "2",
            "maxspeed",
            "50",
        ] {
            string_table.mut_s().push(value.as_bytes().to_vec());
        }

        let mut dense_nodes = osmformat::DenseNodes::new();
        let mut previous_id = 0_i64;
        let mut previous_lat = 0_i64;
        let mut previous_lon = 0_i64;
        for (id, lat, lon) in [
            (1_i64, 480_000_000_i64, 80_000_000_i64),
            (2, 480_001_000, 80_001_000),
            (3, 480_002_000, 80_002_000),
            (4, 480_002_000, 80_003_000),
            (5, 480_003_000, 80_003_000),
        ] {
            dense_nodes.id.push(id - previous_id);
            dense_nodes.lat.push(lat - previous_lat);
            dense_nodes.lon.push(lon - previous_lon);
            previous_id = id;
            previous_lat = lat;
            previous_lon = lon;
        }
        dense_nodes.keys_vals = vec![0, 0, 0, 0, 0];

        let mut road = osmformat::Way::new();
        road.set_id(10);
        road.keys = vec![1, 3, 10, 12];
        road.vals = vec![2, 4, 11, 13];
        road.refs = vec![1, 1, 1];

        let mut building = osmformat::Way::new();
        building.set_id(20);
        building.keys = vec![5, 3, 8];
        building.vals = vec![6, 7, 9];
        building.refs = vec![3, 1, 1, -2];

        let mut group = osmformat::PrimitiveGroup::new();
        group.set_dense(dense_nodes);
        group.mut_ways().push(road);
        group.mut_ways().push(building);

        let mut block = osmformat::PrimitiveBlock::new();
        block.set_stringtable(string_table);
        block.mut_primitivegroup().push(group);

        let mut bytes = Vec::new();
        write_raw_blob(&mut bytes, "OSMData", block.write_to_bytes().unwrap());
        bytes
    }

    fn write_raw_blob(writer: &mut Vec<u8>, field_type: &str, payload: Vec<u8>) {
        let mut blob = fileformat::Blob::new();
        blob.set_raw(payload);
        let blob_bytes = blob.write_to_bytes().unwrap();

        let mut header = fileformat::BlobHeader::new();
        header.set_field_type(field_type.to_owned());
        header.set_datasize(blob_bytes.len().try_into().unwrap());
        let header_bytes = header.write_to_bytes().unwrap();

        let header_len: u32 = header_bytes.len().try_into().unwrap();
        writer.write_all(&header_len.to_be_bytes()).unwrap();
        writer.write_all(&header_bytes).unwrap();
        writer.write_all(&blob_bytes).unwrap();
    }

    #[test]
    fn real_parser_boundary_produces_canonical_game_semantics_and_provenance() {
        let bytes = synthetic_pbf_bytes();
        let first = import_osm_pbf_bytes("synthetic.osm.pbf", &bytes).unwrap();
        let second = import_osm_pbf_bytes("synthetic.osm.pbf", &bytes).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.provenance.parser.revision, GEO_ANALYSIS_REVISION);
        assert_eq!(first.provenance.source_sha256.len(), 64);
        assert_eq!(first.roads.len(), 1);
        assert_eq!(first.roads[0].id, "imported/way/10");
        assert_eq!(first.roads[0].source_id, "way/10");
        assert_eq!(first.roads[0].class, RoadClass::Residential);
        assert_eq!(first.roads[0].lanes, Some(2));
        assert_eq!(first.roads[0].max_speed_kph, Some(50));
        assert_eq!(first.buildings.len(), 1);
        assert_eq!(first.buildings[0].use_kind, BuildingUse::Residential);
        assert_eq!(first.buildings[0].levels, Some(4));
        assert!(matches!(
            first.buildings[0].footprint,
            Geometry::Polygon { .. }
        ));
        assert!(first.buildings[0].gross_floor_area_m2 > 0);

        let encoded = serde_json::to_string(&first).unwrap();
        assert!(!encoded.contains("\"tags\""));
        assert!(!encoded.contains("\"highway\""));
        assert!(!encoded.contains("\"building:levels\""));
        assert!(encoded.contains("grossFloorAreaM2"));
    }

    #[test]
    fn footprint_area_uses_the_short_path_across_the_antimeridian() {
        let crossing = Geometry::Polygon {
            coordinates: vec![vec![
                [179.9999, 0.0],
                [-179.9999, 0.0],
                [-179.9999, 0.0001],
                [179.9999, 0.0001],
                [179.9999, 0.0],
            ]],
        };

        let area = geometry_area_m2(&crossing);
        assert!((200..=300).contains(&area), "unexpected area: {area}");
    }

    #[test]
    fn transit_tags_take_priority_over_highway_fallback() {
        let mut tagged_platform = OsmTags::default();
        tagged_platform.insert("highway".to_owned(), "bus_stop".to_owned());
        tagged_platform.insert("public_transport".to_owned(), "platform".to_owned());
        assert_eq!(classify(&tagged_platform), ImportedKind::Transit);
        assert_eq!(transit_kind(&tagged_platform), TransitKind::BusStop);

        let mut legacy_bus_stop = OsmTags::default();
        legacy_bus_stop.insert("highway".to_owned(), "bus_stop".to_owned());
        assert_eq!(classify(&legacy_bus_stop), ImportedKind::Transit);
    }

    #[test]
    fn highway_point_controls_are_not_imported_as_roads() {
        let mut traffic_signal = OsmTags::default();
        traffic_signal.insert("highway".to_owned(), "traffic_signals".to_owned());
        assert_eq!(classify(&traffic_signal), ImportedKind::Ignore);

        let mut residential = OsmTags::default();
        residential.insert("highway".to_owned(), "residential".to_owned());
        assert_eq!(classify(&residential), ImportedKind::Road);
        assert_eq!(road_class(&residential), RoadClass::Residential);
    }
}
