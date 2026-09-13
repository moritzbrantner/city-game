use std::fmt::Write as _;

use geo_io_osm::{
    CollectOsmBytesOptions, IndexOptions, OsmElementType, OsmFeature, OsmFilterSpec, OsmTags,
    collect_osm_pbf_bytes,
};
use sha2::{Digest, Sha256};

use crate::{
    CityScenario, ExternalRevision, SCENARIO_SCHEMA_VERSION, ScenarioFeature, ScenarioFeatureKind,
    ScenarioProvenance,
};

pub const GEO_ANALYSIS_REVISION: &str = "c4df63a023f2183d700a9a28071d732d345e7c25";
pub const OSM_PARSER_REPOSITORY: &str = "https://github.com/moritzbrantner/geo-analysis";

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

    let mut features = collected
        .features
        .into_iter()
        .map(normalize_feature)
        .collect::<Vec<_>>();
    features.sort_by(|left, right| left.source_id.cmp(&right.source_id));

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
        features,
    })
}

fn normalize_feature(feature: OsmFeature) -> ScenarioFeature {
    let kind = classify(&feature.tags);
    ScenarioFeature {
        source_id: feature.stable_id(),
        kind,
        tags: feature.tags,
        geometry: feature.geometry,
    }
}

fn classify(tags: &OsmTags) -> ScenarioFeatureKind {
    if tags.contains_key("highway") {
        ScenarioFeatureKind::Road
    } else if tags.contains_key("building") {
        ScenarioFeatureKind::Building
    } else if tags.get("natural").is_some_and(|value| value == "water")
        || tags.contains_key("water")
        || tags.contains_key("waterway")
    {
        ScenarioFeatureKind::Water
    } else if tags.contains_key("landuse") {
        ScenarioFeatureKind::LandUse
    } else if tags.contains_key("railway") || tags.contains_key("public_transport") {
        ScenarioFeatureKind::Transit
    } else {
        ScenarioFeatureKind::Other
    }
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
            "yes",
            "Synthetic Building",
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
        road.keys = vec![1, 3];
        road.vals = vec![2, 4];
        road.refs = vec![1, 1, 1];

        let mut building = osmformat::Way::new();
        building.set_id(20);
        building.keys = vec![5, 3];
        building.vals = vec![6, 7];
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
    fn real_parser_boundary_produces_stable_game_features_and_provenance() {
        let bytes = synthetic_pbf_bytes();
        let first = import_osm_pbf_bytes("synthetic.osm.pbf", &bytes).unwrap();
        let second = import_osm_pbf_bytes("synthetic.osm.pbf", &bytes).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.provenance.parser.revision, GEO_ANALYSIS_REVISION);
        assert_eq!(first.provenance.source_sha256.len(), 64);
        assert!(first.features.iter().any(|feature| {
            feature.source_id == "way/10" && feature.kind == ScenarioFeatureKind::Road
        }));
        assert!(first.features.iter().any(|feature| {
            feature.source_id == "way/20" && feature.kind == ScenarioFeatureKind::Building
        }));
        assert!(
            first
                .features
                .windows(2)
                .all(|pair| pair[0].source_id <= pair[1].source_id)
        );
    }
}
