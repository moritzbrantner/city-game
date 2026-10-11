//! Acceptance contract for moritzbrantner/city-game#53: incremental render preparation.
//!
//! Independent acceptance (authored before implementation). The simple full composition
//! (`build_save_render_frame` / `build_save_render_frame_with_view`) is the semantic reference
//! oracle (coding-agent-conventions PRINCIPLE-010 / TEST-022): it shares no retained state with
//! the incremental path. After every command in deterministic sequences the incremental prepared
//! frame must equal the oracle exactly (node identity/order, geometry, colors, opacity,
//! transforms, overview camera, inspection cameras, picking-visible entity IDs).
//!
//! Seams this contract requires (names are the contract; internals are free):
//!
//! * `city_game_core::PreparedCityRender` — derived, disposable render state for one save + aspect.
//!   * `PreparedCityRender::prepare(&CitySave, aspect) -> Result<Self, E: Debug>` — the full
//!     preparation path (simple fallback), also used after save/load.
//!   * `apply(&mut self, &CitySave, &impact) -> Result<_, E: Debug>` — incrementally applies the
//!     explicit render impact of one executed command to the already prepared state.
//!   * `frame(&self) -> &RendererFrame` — prepared nodes plus the fitted overview camera.
//!   * `camera(&self, RenderView) -> Result<RendererCamera, E: Debug>` — camera-only inspection.
//!   * `last_work(&self)` — deterministic work counters of the most recent `prepare`/`apply`,
//!     exposing at least the `u64` fields read by `Work::of` below.
//! * `CitySave::execute_with_render_impact(CityCommand) -> Result<(CityCommandOutcome, impact),
//!   CityCommandError>` — the same validated command gateway as `CitySave::execute`, additionally
//!   returning the explicit render-impact/invalidation result of the outcome.
//!
//! Work-counter semantics:
//! * `full_rebuilds`: whole-scene rebuilds equivalent to the full reference composition.
//! * `entities_visited`: scenario/planning entities read from the save to derive nodes.
//! * `nodes_built`: renderer nodes (re)materialized; `nodes_removed`: retained nodes dropped.
//! * `full_sorts`: sorts of the complete node list.
//! * `overview_refits`: recomputations of overview bounds / overview camera fitting.
//!
//! Inference recorded in the PR: the current reference projects geography around the centre of
//! the geographic bounds of scenario + player roads + zones. A planning change that moves those
//! bounds therefore legitimately changes every node translation; such changes must remain exactly
//! equivalent but may fall back to a full rebuild. Suppressing scenario entities never moves that
//! origin, so it may refit the overview camera but must not rebuild nodes.

use city_game_core::{
    BuildingUse, CityCommand, CityCommandOutcome, CitySave, CityScenario, ExternalRevision,
    LandUseKind, PlannedRoad, PlannedZone, PlanningCommand, PlanningOutcome, PreparedCityRender,
    RenderView, RendererFrame, RoadClass, SCENARIO_SCHEMA_VERSION, ScenarioBuilding,
    ScenarioLandUse, ScenarioProvenance, ScenarioRoad, ScenarioWater, WaterKind, ZoneKind,
    build_save_render_frame, build_save_render_frame_with_view,
};
use geo_core::Geometry;

const ASPECTS: [f32; 2] = [1.0, 16.0 / 9.0];
const SMALL_GRID: usize = 8; // 64 buildings
const LARGE_GRID: usize = 40; // 1,600 buildings
/// Generous constant bound for one localized planning change (a few entities / nodes).
const MAX_LOCAL_WORK: u64 = 8;

const LON0: f64 = 8.4;
const LAT0: f64 = 49.0;
const STEP: f64 = 0.0005;
const SIZE: f64 = 0.0002;

fn views() -> [RenderView; 4] {
    [
        RenderView::overview(),
        RenderView {
            pan_x: 0.25,
            pan_y: -0.125,
            zoom: 2.0,
        },
        RenderView {
            pan_x: -8.0,
            pan_y: 8.0,
            zoom: 0.5,
        },
        RenderView {
            pan_x: 8.0,
            pan_y: -8.0,
            zoom: 32.0,
        },
    ]
}

// ---------------------------------------------------------------------------------------------
// Deterministic synthetic city (the same shape at two sizes).

fn square(lon: f64, lat: f64, size: f64) -> Geometry {
    Geometry::Polygon {
        coordinates: vec![vec![
            [lon, lat],
            [lon + size, lat],
            [lon + size, lat + size],
            [lon, lat + size],
            [lon, lat],
        ]],
    }
}

fn building_id(column: usize, row: usize) -> String {
    format!("imported/building/{column:03}-{row:03}")
}

fn city(grid: usize) -> CitySave {
    let mut buildings = Vec::new();
    for column in 0..grid {
        for row in 0..grid {
            buildings.push(ScenarioBuilding {
                id: building_id(column, row),
                source_id: format!("way/b{column}-{row}"),
                footprint: square(LON0 + column as f64 * STEP, LAT0 + row as f64 * STEP, SIZE),
                use_kind: if (column + row) % 3 == 0 {
                    BuildingUse::Commercial
                } else {
                    BuildingUse::Residential
                },
                name: None,
                levels: Some(((column * 7 + row) % 6 + 1) as u16),
                height_m: None,
                gross_floor_area_m2: 1_000 + (column * grid + row) as u64,
            });
        }
    }
    // Streets between building rows stay strictly inside the building grid's extent, so the
    // (0, 0) building owns the minimum longitude/latitude corner of the city.
    let roads = (0..grid.saturating_sub(1))
        .map(|row| ScenarioRoad {
            id: format!("imported/road/{row:03}"),
            source_id: format!("way/r{row}"),
            geometry: Geometry::LineString {
                coordinates: vec![
                    [LON0 + SIZE * 0.5, LAT0 + row as f64 * STEP + 0.00035],
                    [
                        LON0 + (grid - 1) as f64 * STEP + SIZE * 0.5,
                        LAT0 + row as f64 * STEP + 0.00035,
                    ],
                ],
            },
            class: RoadClass::Residential,
            name: None,
            lanes: Some(2),
            max_speed_kph: Some(50),
        })
        .collect();
    let mid = grid as f64 * STEP * 0.5;
    let scenario = CityScenario {
        schema_version: SCENARIO_SCHEMA_VERSION,
        provenance: ScenarioProvenance {
            source_format: "fixture".to_owned(),
            source_name: format!("incremental-render-{grid}"),
            source_sha256: "incremental-render".to_owned(),
            parser: ExternalRevision {
                repository: "fixture".to_owned(),
                revision: "fixture".to_owned(),
            },
        },
        roads,
        buildings,
        water: vec![ScenarioWater {
            id: "imported/water/pond".to_owned(),
            source_id: "way/w1".to_owned(),
            geometry: square(LON0 + mid + 0.00022, LAT0 + mid + 0.00022, 0.0002),
            kind: WaterKind::Body,
        }],
        land_use_areas: vec![ScenarioLandUse {
            id: "imported/landuse/park".to_owned(),
            source_id: "way/l1".to_owned(),
            geometry: square(LON0 + 0.0003, LAT0 + mid, 0.0001),
            kind: LandUseKind::Recreation,
        }],
        transit_anchors: Vec::new(),
    };
    CitySave::new(scenario).expect("synthetic scenario is a valid current save")
}

fn centre(grid: usize) -> (f64, f64) {
    let middle = (grid / 2) as f64 * STEP;
    (LON0 + middle, LAT0 + middle)
}

// ---------------------------------------------------------------------------------------------
// Commands supported by the current planning gateway.

fn add_road(id: &str, from: [f64; 2], to: [f64; 2]) -> CityCommand {
    CityCommand::Planning {
        command: PlanningCommand::AddRoad {
            road: PlannedRoad {
                id: id.to_owned(),
                geometry: Geometry::LineString {
                    coordinates: vec![from, to],
                },
                class: RoadClass::Secondary,
                name: None,
            },
        },
    }
}

fn interior_road(grid: usize, id: &str) -> CityCommand {
    let (lon, lat) = centre(grid);
    add_road(
        id,
        [lon + 0.00025, lat + 0.0001],
        [lon + 0.00045, lat + 0.0003],
    )
}

fn outlying_road(grid: usize) -> CityCommand {
    let far = LON0 + grid as f64 * STEP + 0.01;
    add_road(
        "player/road/outlying",
        [far, LAT0 + 0.0001],
        [far + 0.002, LAT0 + 0.0011],
    )
}

fn remove_road(id: &str) -> CityCommand {
    CityCommand::Planning {
        command: PlanningCommand::RemovePlayerRoad { id: id.to_owned() },
    }
}

fn zone(grid: usize, id: &str, kind: ZoneKind, size: f64) -> CityCommand {
    let (lon, lat) = centre(grid);
    CityCommand::Planning {
        command: PlanningCommand::ZoneArea {
            zone: PlannedZone {
                id: id.to_owned(),
                geometry: square(lon - 0.00025, lat - 0.00025, size),
                kind,
            },
        },
    }
}

fn remove_zone(id: &str) -> CityCommand {
    CityCommand::Planning {
        command: PlanningCommand::RemoveZone { id: id.to_owned() },
    }
}

fn suppress(id: &str) -> CityCommand {
    CityCommand::Planning {
        command: PlanningCommand::SuppressScenarioEntity { id: id.to_owned() },
    }
}

fn restore(id: &str) -> CityCommand {
    CityCommand::Planning {
        command: PlanningCommand::RestoreScenarioEntity { id: id.to_owned() },
    }
}

fn interior_building(grid: usize) -> String {
    building_id(grid / 2, grid / 2)
}

fn corner_building() -> String {
    building_id(0, 0)
}

// ---------------------------------------------------------------------------------------------
// Differential harness.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Work {
    full_rebuilds: u64,
    entities_visited: u64,
    nodes_built: u64,
    nodes_removed: u64,
    full_sorts: u64,
    overview_refits: u64,
}

impl Work {
    fn of(prepared: &PreparedCityRender) -> Self {
        let work = prepared.last_work();
        Self {
            full_rebuilds: work.full_rebuilds,
            entities_visited: work.entities_visited,
            nodes_built: work.nodes_built,
            nodes_removed: work.nodes_removed,
            full_sorts: work.full_sorts,
            overview_refits: work.overview_refits,
        }
    }

    const NONE: Self = Self {
        full_rebuilds: 0,
        entities_visited: 0,
        nodes_built: 0,
        nodes_removed: 0,
        full_sorts: 0,
        overview_refits: 0,
    };
}

/// One authoritative save driven through the command gateway, with incremental render state
/// for every tested aspect.
struct Session {
    save: CitySave,
    prepared: Vec<(f32, PreparedCityRender)>,
}

impl Session {
    fn new(save: CitySave) -> Self {
        let prepared = ASPECTS
            .iter()
            .map(|&aspect| {
                let prepared =
                    PreparedCityRender::prepare(&save, aspect).expect("full preparation succeeds");
                (aspect, prepared)
            })
            .collect();
        let session = Self { save, prepared };
        session.assert_matches_oracle("initial full preparation");
        session
    }

    /// Execute through the command gateway; apply the explicit render impact incrementally.
    /// Returns the outcome (or `None` when the command is rejected) and the work of the first aspect.
    fn step(&mut self, command: CityCommand) -> (Option<CityCommandOutcome>, Work) {
        let label = format!("{command:?}");
        let before = self.save.clone();
        let outcome = match self.save.execute_with_render_impact(command) {
            Ok((outcome, impact)) => {
                for (_, prepared) in &mut self.prepared {
                    prepared
                        .apply(&self.save, &impact)
                        .expect("incremental application succeeds");
                }
                Some(outcome)
            }
            Err(_) => {
                assert_eq!(
                    self.save, before,
                    "rejected command mutated the save: {label}"
                );
                None
            }
        };
        self.assert_matches_oracle(&label);
        (outcome, Work::of(&self.prepared[0].1))
    }

    fn frame(&self) -> &RendererFrame {
        self.prepared[0].1.frame()
    }

    fn assert_matches_oracle(&self, label: &str) {
        for (aspect, prepared) in &self.prepared {
            let reference = build_save_render_frame(&self.save, *aspect)
                .expect("reference full composition succeeds");
            let actual = prepared.frame();
            assert_eq!(
                node_ids(actual),
                node_ids(&reference),
                "node identity/order diverged after {label} (aspect {aspect})"
            );
            assert_eq!(
                picking_entities(actual),
                picking_entities(&reference),
                "picking-visible entity IDs diverged after {label} (aspect {aspect})"
            );
            assert_eq!(
                actual.nodes, reference.nodes,
                "renderer geometry/transforms diverged after {label} (aspect {aspect})"
            );
            assert_eq!(
                actual.camera, reference.camera,
                "overview camera diverged after {label} (aspect {aspect})"
            );
            assert_eq!(actual, &reference, "complete frame diverged after {label}");
            for view in views() {
                let reference_view = build_save_render_frame_with_view(&self.save, *aspect, view)
                    .expect("reference view succeeds");
                assert_eq!(
                    prepared.camera(view).expect("prepared camera succeeds"),
                    reference_view.camera,
                    "camera for {view:?} diverged after {label} (aspect {aspect})"
                );
            }
        }
    }
}

fn node_ids(frame: &RendererFrame) -> Vec<&str> {
    frame.nodes.iter().map(|node| node.id.as_str()).collect()
}

/// Mirrors the browser's node-to-entity mapping (`entityIdForNode`): road segments pick their road.
fn picking_entities(frame: &RendererFrame) -> Vec<&str> {
    let mut ids: Vec<&str> = frame
        .nodes
        .iter()
        .map(|node| match node.id.rfind("/road-segment-") {
            Some(index) => &node.id[..index],
            None => node.id.as_str(),
        })
        .collect();
    ids.dedup();
    ids
}

fn oracle_overview(save: &CitySave) -> RendererFrame {
    build_save_render_frame(save, ASPECTS[0]).unwrap()
}

fn assert_local(work: Work, label: &str) {
    assert_eq!(
        work.full_rebuilds, 0,
        "{label}: rebuilt the whole scene ({work:?})"
    );
    assert_eq!(
        work.full_sorts, 0,
        "{label}: re-sorted the whole scene ({work:?})"
    );
    assert_eq!(
        work.overview_refits, 0,
        "{label}: refit unaffected bounds ({work:?})"
    );
    assert!(
        work.entities_visited <= MAX_LOCAL_WORK,
        "{label}: visited too many entities ({work:?})"
    );
    assert!(
        work.nodes_built + work.nodes_removed <= MAX_LOCAL_WORK,
        "{label}: touched too many nodes ({work:?})"
    );
}

// ---------------------------------------------------------------------------------------------
// Correctness: differential oracle after every step.

#[test]
fn localized_planning_changes_match_full_reference_after_every_step() {
    let grid = SMALL_GRID;
    let mut session = Session::new(city(grid));
    let initial = session.frame().clone();
    let road = "player/road/a";
    let changed_road = add_road(
        road,
        [centre(grid).0 + 0.0001, centre(grid).1 + 0.00035],
        [centre(grid).0 + 0.0004, centre(grid).1 + 0.00036],
    );
    for command in [
        interior_road(grid, road),
        zone(grid, "player/zone/a", ZoneKind::Residential, 0.0003),
        suppress(&interior_building(grid)),
        suppress("imported/road/003"),
        suppress("imported/water/pond"),
        suppress("imported/landuse/park"),
        // "Change" is remove + re-add under the same id (the gateway rejects in-place conflicts).
        remove_road(road),
        changed_road,
        remove_zone("player/zone/a"),
        zone(grid, "player/zone/a", ZoneKind::Industrial, 0.0002),
        zone(grid, "player/zone/b", ZoneKind::MixedUse, 0.0001),
        restore(&interior_building(grid)),
        restore("imported/road/003"),
        restore("imported/water/pond"),
        restore("imported/landuse/park"),
        remove_zone("player/zone/a"),
        remove_zone("player/zone/b"),
        remove_road(road),
    ] {
        let (outcome, _) = session.step(command);
        assert_eq!(
            outcome,
            Some(CityCommandOutcome::Planning(PlanningOutcome::Applied))
        );
    }
    assert_eq!(
        session.frame(),
        &initial,
        "undoing every change restores the initial frame"
    );
}

#[test]
fn bounds_affecting_changes_and_restart_match_full_reference() {
    let grid = SMALL_GRID;
    let mut session = Session::new(city(grid));
    let initial = session.frame().clone();

    // Outlying road: moves geographic bounds and the fitted overview.
    let before = oracle_overview(&session.save);
    let (_, work) = session.step(outlying_road(grid));
    let after = oracle_overview(&session.save);
    assert_ne!(
        before.camera, after.camera,
        "fixture must move the overview"
    );
    assert_ne!(
        before.nodes[0], after.nodes[0],
        "fixture must move the projection origin"
    );
    assert_eq!(
        work.overview_refits, 1,
        "bounds-affecting change refits once ({work:?})"
    );

    session.step(remove_road("player/road/outlying"));
    assert_eq!(session.frame(), &initial);

    // Corner building: the overview shrinks/grows, but node transforms stay valid.
    let before = oracle_overview(&session.save);
    let (_, work) = session.step(suppress(&corner_building()));
    let after = oracle_overview(&session.save);
    assert_ne!(
        before.camera, after.camera,
        "fixture must change the fitted overview"
    );
    assert_eq!(
        work.overview_refits, 1,
        "boundary suppression refits once ({work:?})"
    );
    assert_eq!(
        work.full_rebuilds, 0,
        "suppression keeps node transforms ({work:?})"
    );
    assert!(
        work.nodes_built + work.nodes_removed <= MAX_LOCAL_WORK,
        "{work:?}"
    );
    let (_, work) = session.step(restore(&corner_building()));
    assert_eq!(
        work.overview_refits, 1,
        "boundary restoration refits once ({work:?})"
    );
    assert_eq!(
        work.full_rebuilds, 0,
        "restoration keeps node transforms ({work:?})"
    );
    assert_eq!(session.frame(), &initial);

    // Restart after mixed edits, including bounds-affecting ones.
    for command in [
        interior_road(grid, "player/road/a"),
        outlying_road(grid),
        zone(grid, "player/zone/a", ZoneKind::Commercial, 0.0003),
        suppress(&corner_building()),
        suppress(&interior_building(grid)),
    ] {
        session.step(command);
    }
    let (outcome, _) = session.step(CityCommand::Restart);
    assert_eq!(outcome, Some(CityCommandOutcome::Restarted));
    assert_eq!(
        session.frame(),
        &initial,
        "restart restores the scenario frame"
    );
    let (outcome, _) = session.step(CityCommand::Restart);
    assert_eq!(outcome, Some(CityCommandOutcome::Restarted));
    assert_eq!(session.frame(), &initial);
}

#[test]
fn repeated_noop_and_rejected_commands_do_no_render_work() {
    let grid = SMALL_GRID;
    let mut session = Session::new(city(grid));
    session.step(interior_road(grid, "player/road/a"));
    session.step(zone(grid, "player/zone/a", ZoneKind::Residential, 0.0003));
    session.step(suppress(&corner_building()));
    let frame = session.frame().clone();

    for command in [
        interior_road(grid, "player/road/a"),
        zone(grid, "player/zone/a", ZoneKind::Residential, 0.0003),
        suppress(&corner_building()),
        restore(&interior_building(grid)),
        remove_road("player/road/missing"),
        remove_zone("player/zone/missing"),
    ] {
        let label = format!("{command:?}");
        let (outcome, work) = session.step(command);
        assert_eq!(
            outcome,
            Some(CityCommandOutcome::Planning(PlanningOutcome::Unchanged)),
            "{label}"
        );
        assert_eq!(
            work,
            Work::NONE,
            "authoritative no-op did render work: {label}"
        );
        assert_eq!(session.frame(), &frame, "{label}");
    }

    for command in [
        // Conflicting change under an existing id, reserved scenario id, unknown entity.
        zone(grid, "player/zone/a", ZoneKind::Industrial, 0.0003),
        add_road(
            &building_id(1, 1),
            [LON0, LAT0],
            [LON0 + 0.0001, LAT0 + 0.0001],
        ),
        suppress("imported/building/missing"),
    ] {
        let label = format!("{command:?}");
        let (outcome, _) = session.step(command);
        assert_eq!(outcome, None, "command must be rejected: {label}");
        assert_eq!(session.frame(), &frame, "{label}");
    }
}

#[test]
fn save_load_boundary_preserves_equivalence_and_full_fallback() {
    let grid = SMALL_GRID;
    let mut session = Session::new(city(grid));
    for command in [
        interior_road(grid, "player/road/a"),
        outlying_road(grid),
        zone(grid, "player/zone/a", ZoneKind::Commercial, 0.0003),
        suppress(&corner_building()),
        suppress(&interior_building(grid)),
    ] {
        session.step(command);
    }

    let encoded = serde_json::to_string(&session.save).unwrap();
    let loaded: CitySave = serde_json::from_str(&encoded).unwrap();
    let mut resumed = Session::new(loaded);
    for (aspect, prepared) in &resumed.prepared {
        let continuing = &session
            .prepared
            .iter()
            .find(|(a, _)| a == aspect)
            .unwrap()
            .1;
        assert_eq!(
            prepared.frame(),
            continuing.frame(),
            "loaded preparation (aspect {aspect})"
        );
        let work = Work::of(prepared);
        assert_eq!(
            work.full_rebuilds, 1,
            "full preparation path remains available ({work:?})"
        );
        assert_eq!(work.full_sorts, 1, "{work:?}");
        assert_eq!(work.overview_refits, 1, "{work:?}");
    }

    // Both the continuing and the resumed incremental state keep matching the oracle.
    for command in [
        remove_road("player/road/outlying"),
        restore(&corner_building()),
        remove_zone("player/zone/a"),
        interior_road(grid, "player/road/b"),
        CityCommand::Restart,
    ] {
        session.step(command.clone());
        resumed.step(command);
        assert_eq!(session.frame(), resumed.frame());
    }

    assert!(PreparedCityRender::prepare(&session.save, 0.0).is_err());
    assert!(PreparedCityRender::prepare(&session.save, f32::NAN).is_err());
}

#[test]
fn fixed_seed_mixed_journey_matches_full_reference_after_every_step() {
    let grid = SMALL_GRID;
    let mut session = Session::new(city(grid));
    let pool = vec![
        interior_road(grid, "player/road/a"),
        interior_road(grid, "player/road/b"),
        remove_road("player/road/a"),
        remove_road("player/road/b"),
        outlying_road(grid),
        remove_road("player/road/outlying"),
        zone(grid, "player/zone/a", ZoneKind::Residential, 0.0003),
        zone(grid, "player/zone/a", ZoneKind::Commercial, 0.0002),
        remove_zone("player/zone/a"),
        zone(grid, "player/zone/b", ZoneKind::Industrial, 0.0001),
        remove_zone("player/zone/b"),
        suppress(&corner_building()),
        restore(&corner_building()),
        suppress(&interior_building(grid)),
        restore(&interior_building(grid)),
        suppress("imported/road/000"),
        restore("imported/road/000"),
        suppress("imported/water/pond"),
        restore("imported/water/pond"),
        CityCommand::Restart,
    ];
    let mut state: u64 = 0x5eed_0053;
    let mut applied = 0;
    for _ in 0..120 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let command = pool[(state >> 33) as usize % pool.len()].clone();
        if matches!(
            session.step(command).0,
            Some(CityCommandOutcome::Planning(PlanningOutcome::Applied))
        ) {
            applied += 1;
        }
    }
    assert!(
        applied >= 30,
        "journey must exercise real mutations, got {applied}"
    );
}

// ---------------------------------------------------------------------------------------------
// Performance: deterministic work counters, independent of city size.

#[test]
fn localized_mutations_do_bounded_work_independent_of_city_size() {
    let mut per_size = Vec::new();
    for grid in [SMALL_GRID, LARGE_GRID] {
        let mut session = Session::new(city(grid));
        let prepared_nodes = session.frame().nodes.len() as u64;
        let full = Work::of(&session.prepared[0].1);
        assert_eq!(
            full.full_rebuilds, 1,
            "initial preparation is the full path ({full:?})"
        );

        let mut works = Vec::new();
        for command in [
            interior_road(grid, "player/road/a"),
            zone(grid, "player/zone/a", ZoneKind::Residential, 0.0003),
            suppress(&interior_building(grid)),
            restore(&interior_building(grid)),
            remove_zone("player/zone/a"),
            zone(grid, "player/zone/a", ZoneKind::Industrial, 0.0002),
            remove_road("player/road/a"),
        ] {
            let label = format!("{command:?} on {grid}x{grid}");
            let camera_before = oracle_overview(&session.save).camera;
            let (outcome, work) = session.step(command);
            assert_eq!(
                outcome,
                Some(CityCommandOutcome::Planning(PlanningOutcome::Applied)),
                "{label}"
            );
            assert_eq!(
                oracle_overview(&session.save).camera,
                camera_before,
                "fixture change must not affect bounds: {label}"
            );
            assert_local(work, &label);
            assert!(
                work.nodes_built + work.nodes_removed > 0,
                "{label}: no node was updated"
            );
            works.push(work);
        }
        per_size.push((grid, prepared_nodes, works));
    }

    let (small_grid, small_nodes, small) = &per_size[0];
    let (large_grid, large_nodes, large) = &per_size[1];
    assert!(
        large_nodes >= &(small_nodes * 16),
        "workloads must differ substantially in size"
    );
    assert_eq!(
        small, large,
        "localized work changed with city size ({small_grid}x{small_grid} vs {large_grid}x{large_grid})"
    );
}

#[test]
fn camera_only_navigation_does_not_change_prepared_work() {
    let session = Session::new(city(LARGE_GRID));
    let prepared = &session.prepared[0].1;
    let work = Work::of(prepared);
    let nodes = prepared.frame().nodes.as_ptr();
    for step in 0..256 {
        let view = RenderView {
            pan_x: f32::from(step as u8 % 17) / 4.0 - 2.0,
            pan_y: 0.0,
            zoom: 2.0,
        };
        prepared.camera(view).unwrap();
    }
    assert_eq!(Work::of(prepared), work);
    assert_eq!(
        prepared.frame().nodes.as_ptr(),
        nodes,
        "camera-only use kept node storage"
    );
}
