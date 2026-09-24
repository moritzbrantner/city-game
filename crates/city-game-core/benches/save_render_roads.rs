use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Instant,
};

use city_game_core::{
    CitySave, CityScenario, ExternalRevision, RoadClass, SCENARIO_SCHEMA_VERSION,
    ScenarioProvenance, ScenarioRoad, build_save_render_frame,
};
use geo_core::Geometry;
use serde_json::json;

struct CountingAllocator;

static TRACKING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_allocation(new_size);
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

fn record_allocation(bytes: usize) {
    if TRACKING.load(Ordering::Relaxed) {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(bytes as u64, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, Copy)]
struct Sample {
    elapsed_ns: u64,
    allocations: u64,
    allocated_bytes: u64,
}

fn main() {
    let smoke = std::env::args().any(|argument| argument == "--smoke");
    let (roads, vertices, samples) = if smoke {
        (2_048, 16, 7)
    } else {
        (8_192, 32, 9)
    };
    let save = CitySave::new(synthetic_scenario(roads, vertices)).unwrap();
    let expected_nodes = roads * (vertices - 1);

    for _ in 0..2 {
        black_box(build_save_render_frame(&save, 16.0 / 9.0).unwrap());
    }

    let measurements: Vec<_> = (0..samples)
        .map(|_| measure_render(&save, expected_nodes))
        .collect();
    let elapsed: Vec<_> = measurements
        .iter()
        .map(|sample| sample.elapsed_ns)
        .collect();
    let allocation_counts: Vec<_> = measurements
        .iter()
        .map(|sample| sample.allocations)
        .collect();
    let allocated_bytes: Vec<_> = measurements
        .iter()
        .map(|sample| sample.allocated_bytes)
        .collect();

    println!(
        "{}",
        json!({
            "benchmark": "save-render-road-materialization",
            "mode": if smoke { "smoke" } else { "full" },
            "roads": roads,
            "verticesPerRoad": vertices,
            "renderNodes": expected_nodes,
            "samples": samples,
            "medianNs": median(&elapsed),
            "p95Ns": percentile95(&elapsed),
            "medianAllocationCalls": median(&allocation_counts),
            "medianAllocatedBytes": median(&allocated_bytes),
            "elapsedSamplesNs": elapsed,
            "allocationCallSamples": allocation_counts,
            "allocatedByteSamples": allocated_bytes,
            "timing": "advisory-host-local",
            "allocationCounts": "deterministic-hot-path-observation"
        })
    );
}

fn measure_render(save: &CitySave, expected_nodes: usize) -> Sample {
    ALLOCATIONS.store(0, Ordering::Relaxed);
    ALLOCATED_BYTES.store(0, Ordering::Relaxed);

    let start = Instant::now();
    TRACKING.store(true, Ordering::SeqCst);
    let frame = black_box(build_save_render_frame(save, 16.0 / 9.0).unwrap());
    TRACKING.store(false, Ordering::SeqCst);
    let elapsed_ns = u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);

    assert_eq!(frame.nodes.len(), expected_nodes);

    Sample {
        elapsed_ns,
        allocations: ALLOCATIONS.load(Ordering::Relaxed),
        allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
    }
}

fn median(values: &[u64]) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

fn percentile95(values: &[u64]) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[(sorted.len() * 95).div_ceil(100).saturating_sub(1)]
}

fn synthetic_scenario(roads: usize, vertices: usize) -> CityScenario {
    CityScenario {
        schema_version: SCENARIO_SCHEMA_VERSION,
        provenance: ScenarioProvenance {
            source_format: "benchmark".to_owned(),
            source_name: "save-render-roads".to_owned(),
            source_sha256: "benchmark".to_owned(),
            parser: ExternalRevision {
                repository: "benchmark".to_owned(),
                revision: "benchmark".to_owned(),
            },
        },
        roads: (0..roads)
            .map(|index| synthetic_road(index, vertices))
            .collect(),
        buildings: Vec::new(),
        water: Vec::new(),
        land_use_areas: Vec::new(),
        transit_anchors: Vec::new(),
    }
}

fn synthetic_road(index: usize, vertices: usize) -> ScenarioRoad {
    let row = index / 64;
    let column = index % 64;
    let base_lon = 8.0 + column as f64 * 0.000_2;
    let base_lat = 48.0 + row as f64 * 0.000_2;
    let coordinates = (0..vertices)
        .map(|vertex| {
            [
                base_lon + vertex as f64 * 0.000_008,
                base_lat + (vertex % 3) as f64 * 0.000_004,
            ]
        })
        .collect();

    ScenarioRoad {
        id: format!("benchmark/road/{index:06}"),
        source_id: format!("road/{index}"),
        geometry: Geometry::LineString { coordinates },
        class: RoadClass::Residential,
        name: None,
        lanes: Some(2),
        max_speed_kph: Some(50),
    }
}
