use std::{hint::black_box, time::Instant};

use city_game_core::{
    BuildingUse, CityRuleset, CitySave, CityScenario, CityTimeConfig, ExternalRevision,
    PopulationCapacity, SCENARIO_SCHEMA_VERSION, ScenarioBuilding, ScenarioProvenance,
};
use geo_core::Geometry;
use serde_json::json;

#[derive(Clone, Copy)]
struct BenchmarkConfig {
    buildings: usize,
    batches: u64,
    steps_per_batch: u64,
    samples: usize,
}

fn main() {
    let smoke = std::env::args().any(|argument| argument == "--smoke");
    let config = if smoke {
        BenchmarkConfig {
            buildings: 10_000,
            batches: 32,
            steps_per_batch: 10_000,
            samples: 7,
        }
    } else {
        BenchmarkConfig {
            buildings: 50_000,
            batches: 64,
            steps_per_batch: 10_000,
            samples: 9,
        }
    };

    let large_scenario = scenario_with_buildings(config.buildings);
    let empty_scenario = scenario_with_buildings(0);
    let ruleset = CityRuleset::default();
    let large_save = CitySave::new_with_ruleset(large_scenario.clone(), ruleset.clone()).unwrap();
    let empty_save = CitySave::new_with_ruleset(empty_scenario, ruleset.clone()).unwrap();

    let empty_candidate = measure_candidate(&empty_save, config);
    let large_candidate = measure_candidate(&large_save, config);
    let legacy_reference = measure_legacy_reference(&large_scenario, &ruleset, config);

    let empty_median = median(&empty_candidate);
    let large_median = median(&large_candidate);
    let legacy_median = median(&legacy_reference);
    let speedup = legacy_median as f64 / large_median.max(1) as f64;

    println!(
        "{}",
        json!({
            "benchmark": "fixed-step-hot-path",
            "mode": if smoke { "smoke" } else { "full" },
            "buildings": config.buildings,
            "batches": config.batches,
            "stepsPerBatch": config.steps_per_batch,
            "totalSteps": config.batches * config.steps_per_batch,
            "candidateEmptyMedianNs": empty_median,
            "candidateLargeMedianNs": large_median,
            "legacyReferenceMedianNs": legacy_median,
            "legacyToCandidateSpeedup": speedup,
            "candidateEmptySamplesNs": empty_candidate,
            "candidateLargeSamplesNs": large_candidate,
            "legacyReferenceSamplesNs": legacy_reference,
            "timing": "advisory-host-local",
            "blocking": false,
        })
    );
}

fn measure_candidate(baseline: &CitySave, config: BenchmarkConfig) -> Vec<u64> {
    (0..config.samples)
        .map(|_| {
            let mut save = baseline.clone();
            let start = Instant::now();
            for _ in 0..config.batches {
                black_box(save.advance_fixed_steps(config.steps_per_batch).unwrap());
            }
            let elapsed = elapsed_ns(start);
            let expected_tick = config
                .batches
                .checked_mul(config.steps_per_batch)
                .expect("benchmark tick count fits u64");
            assert_eq!(save.time_position().unwrap().tick, expected_tick);
            elapsed
        })
        .collect()
}

fn measure_legacy_reference(
    scenario: &CityScenario,
    ruleset: &CityRuleset,
    config: BenchmarkConfig,
) -> Vec<u64> {
    let time = CityTimeConfig::default();
    (0..config.samples)
        .map(|_| {
            let mut tick = 0_u64;
            let start = Instant::now();
            for _ in 0..config.batches {
                ruleset.validate().unwrap();
                if ruleset.population.is_enabled() {
                    black_box(
                        PopulationCapacity::from_scenario(scenario, ruleset.population.config)
                            .unwrap(),
                    );
                }

                let final_tick = tick
                    .checked_add(config.steps_per_batch)
                    .expect("benchmark tick count fits u64");
                let position = time.position(final_tick).unwrap();
                for _ in 0..config.steps_per_batch {
                    tick += 1;
                }
                debug_assert_eq!(tick, final_tick);
                black_box(position);
            }
            let elapsed = elapsed_ns(start);
            assert_eq!(
                tick,
                config
                    .batches
                    .checked_mul(config.steps_per_batch)
                    .expect("benchmark tick count fits u64")
            );
            elapsed
        })
        .collect()
}

fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn median(samples: &[u64]) -> u64 {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

fn scenario_with_buildings(count: usize) -> CityScenario {
    CityScenario {
        schema_version: SCENARIO_SCHEMA_VERSION,
        provenance: ScenarioProvenance {
            source_format: "benchmark".to_owned(),
            source_name: "synthetic-fixed-step".to_owned(),
            source_sha256: "benchmark".to_owned(),
            parser: ExternalRevision {
                repository: "benchmark".to_owned(),
                revision: "benchmark".to_owned(),
            },
        },
        roads: Vec::new(),
        buildings: (0..count).map(building).collect(),
        water: Vec::new(),
        land_use_areas: Vec::new(),
        transit_anchors: Vec::new(),
    }
}

fn building(index: usize) -> ScenarioBuilding {
    ScenarioBuilding {
        id: format!("benchmark/building/{index}"),
        source_id: format!("building/{index}"),
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
        height_m: Some(12.0),
        gross_floor_area_m2: 9_000,
    }
}
