//! Incremental, disposable render preparation (city-game#53).
//!
//! [`PreparedCityRender`] is derived from one [`CitySave`] and one viewport aspect. It is never
//! persisted, never consulted by simulation, and can always be discarded and re-prepared from the
//! save. `CitySave` stays the only authority: every update re-reads the effective state of the
//! entities named by an explicit [`CityRenderImpact`] instead of trusting cached semantics.
//!
//! Retained state is split along existing ownership:
//!
//! * immutable imported scenario entities keep their derived nodes for the lifetime of the
//!   projection. Suppression moves those nodes between the frame and a hidden slot instead of
//!   re-deriving geometry;
//! * mutable planning-overlay entities (player roads and zones) are re-derived from the save when
//!   an impact names them.
//!
//! The full composition ([`super::build_save_render_frame`]) remains the simple reference. The
//! projection origin is the centre of scenario + overlay geographic bounds, so an overlay change
//! that moves that origin changes every translation and deliberately falls back to [`prepare`].
//! Overview fitting is maintained from per-entity summaries and refit only when a change can
//! affect the aggregate bounds.
//!
//! [`prepare`]: PreparedCityRender::prepare

use std::collections::HashMap;

use serde::Serialize;

use super::{
    CameraError, CameraExtents, GameWorldProjection, GeographicBounds, OrthographicCamera,
    RenderFrameError, RenderView, RendererCamera, RendererFrame, RendererSceneNode, Vec3,
    append_building_node, append_land_use_node, append_road_nodes, append_water_node,
    append_zone_node, apply_render_view, extend_geographic_bounds, overview_camera_from_extents,
    overview_eye_distance, overview_orientation, renderer_camera, road_segment_node_id,
    visit_positions, visit_scenario_positions,
};
use crate::commands::RenderInvalidation;
use crate::{CityRenderImpact, CitySave, PlannedRoad, PlannedZone};

/// Deterministic work counters of the most recent [`PreparedCityRender`] operation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderPreparationWork {
    /// Whole-scene rebuilds equivalent to the full reference composition.
    pub full_rebuilds: u64,
    /// Scenario/planning entities whose effective state was read from the save.
    pub entities_visited: u64,
    /// Renderer nodes materialized into the prepared frame.
    pub nodes_built: u64,
    /// Renderer nodes dropped from the prepared frame.
    pub nodes_removed: u64,
    /// Sorts of the complete node list.
    pub full_sorts: u64,
    /// Overview bounds rescans or overview camera fits.
    pub overview_refits: u64,
}

impl RenderPreparationWork {
    /// Adds another operation's counters (used by adapters that batch several operations).
    pub fn accumulate(&mut self, other: Self) {
        self.full_rebuilds += other.full_rebuilds;
        self.entities_visited += other.entities_visited;
        self.nodes_built += other.nodes_built;
        self.nodes_removed += other.nodes_removed;
        self.full_sorts += other.full_sorts;
        self.overview_refits += other.overview_refits;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeKind {
    Road,
    Area,
}

/// Derived render summary of one effective entity.
#[derive(Debug, Clone)]
struct RetainedEntity {
    kind: NodeKind,
    node_count: usize,
    fit_points: Vec<Vec3>,
    /// Largest absolute fit-point coordinate; drives the overview eye distance.
    max_abs: f32,
    /// Overview camera-space extents for the current eye distance.
    extents: Option<CameraExtents>,
    /// Nodes of a suppressed scenario entity, retained outside the frame.
    hidden: Option<Vec<RendererSceneNode>>,
    /// Geographic bounds of an overlay entity (overlay geometry feeds the projection origin).
    geo: Option<GeographicBounds>,
}

impl RetainedEntity {
    fn visible(&self) -> bool {
        self.hidden.is_none()
    }

    fn node_id(&self, entity_id: &str, index: usize) -> String {
        match self.kind {
            NodeKind::Road => road_segment_node_id(entity_id, index),
            NodeKind::Area => entity_id.to_owned(),
        }
    }
}

#[derive(Clone, Copy)]
enum OverlaySource<'a> {
    Road(&'a PlannedRoad),
    Zone(&'a PlannedZone),
}

impl<'a> OverlaySource<'a> {
    fn resolve(save: &'a CitySave, id: &str) -> Option<Self> {
        let planning = &save.world.planning;
        planning
            .player_roads
            .get(id)
            .map(Self::Road)
            .or_else(|| planning.zones.get(id).map(Self::Zone))
    }

    fn geographic_bounds(self) -> Option<GeographicBounds> {
        let geometry = match self {
            Self::Road(road) => &road.geometry,
            Self::Zone(zone) => &zone.geometry,
        };
        let mut bounds = None;
        visit_positions(geometry, &mut |position| {
            extend_geographic_bounds(&mut bounds, position);
        });
        bounds
    }

    fn derive(
        self,
        projection: GameWorldProjection,
        nodes: &mut Vec<RendererSceneNode>,
        fit_points: &mut Vec<Vec3>,
    ) -> NodeKind {
        match self {
            Self::Road(road) => {
                append_road_nodes(
                    &road.id,
                    &road.geometry,
                    road.class,
                    projection,
                    nodes,
                    fit_points,
                );
                NodeKind::Road
            }
            Self::Zone(zone) => {
                append_zone_node(zone, projection, nodes, fit_points);
                NodeKind::Area
            }
        }
    }
}

enum Step {
    Applied,
    Rebuild,
}

/// Retained, incrementally updatable render preparation for one save and aspect.
///
/// Callers must apply the impact of every executed command to keep it consistent with the save;
/// on any error the value must be discarded and re-prepared.
#[derive(Debug, Clone)]
pub struct PreparedCityRender {
    aspect: f32,
    projection: GameWorldProjection,
    scenario_geo: Option<GeographicBounds>,
    geo: Option<GeographicBounds>,
    scenario: HashMap<String, RetainedEntity>,
    overlay: HashMap<String, RetainedEntity>,
    max_abs: f32,
    eye_distance: f32,
    extents: Option<CameraExtents>,
    overview: OrthographicCamera,
    frame: RendererFrame,
    unique_node_ids: bool,
    work: RenderPreparationWork,
}

impl PreparedCityRender {
    /// Full preparation: the simple fallback path, also used after save/load.
    pub fn prepare(save: &CitySave, aspect: f32) -> Result<Self, CameraError> {
        if !aspect.is_finite() || aspect <= 0.0 {
            return Err(CameraError::InvalidAspect);
        }
        let planning = &save.world.planning;
        let mut work = RenderPreparationWork {
            full_rebuilds: 1,
            full_sorts: 1,
            ..RenderPreparationWork::default()
        };

        let mut scenario_geo = None;
        visit_scenario_positions(&save.scenario, &mut |position| {
            extend_geographic_bounds(&mut scenario_geo, position);
        });
        let overlay_sources = planning
            .player_roads
            .values()
            .map(OverlaySource::Road)
            .chain(planning.zones.values().map(OverlaySource::Zone))
            .map(|source| (source, source.geographic_bounds()))
            .collect::<Vec<_>>();
        let geo = overlay_sources
            .iter()
            .fold(scenario_geo, |bounds, (_, geo)| {
                merge_geographic(bounds, *geo)
            });
        let projection = GameWorldProjection::from_bounds(geo);

        // Visible nodes are generated in the reference order (effective roads, buildings, water,
        // land use, zones) so the stable id sort reproduces the reference exactly.
        let mut nodes = Vec::new();
        let mut scenario = HashMap::with_capacity(
            save.scenario.roads.len()
                + save.scenario.buildings.len()
                + save.scenario.water.len()
                + save.scenario.land_use_areas.len(),
        );
        let mut overlay = HashMap::with_capacity(overlay_sources.len());
        let mut retain = |hidden: bool,
                          geo: Option<GeographicBounds>,
                          nodes: &mut Vec<RendererSceneNode>,
                          derive: &mut dyn FnMut(
            &mut Vec<RendererSceneNode>,
            &mut Vec<Vec3>,
        ) -> NodeKind| {
            let mut fit_points = Vec::new();
            let mut hidden_nodes = Vec::new();
            let target = if hidden { &mut hidden_nodes } else { nodes };
            let start = target.len();
            let kind = derive(target, &mut fit_points);
            let node_count = target.len() - start;
            work.entities_visited += 1;
            work.nodes_built += node_count as u64;
            RetainedEntity {
                kind,
                node_count,
                max_abs: max_abs_of(&fit_points),
                fit_points,
                extents: None,
                hidden: hidden.then_some(hidden_nodes),
                geo,
            }
        };

        for road in &save.scenario.roads {
            let entity = retain(
                planning.is_suppressed(&road.id),
                None,
                &mut nodes,
                &mut |nodes, fit_points| {
                    append_road_nodes(
                        &road.id,
                        &road.geometry,
                        road.class,
                        projection,
                        nodes,
                        fit_points,
                    );
                    NodeKind::Road
                },
            );
            scenario.insert(road.id.clone(), entity);
        }
        let (roads, zones): (Vec<_>, Vec<_>) = overlay_sources
            .into_iter()
            .partition(|(source, _)| matches!(source, OverlaySource::Road(_)));
        for (source, geo) in roads {
            let id = overlay_id(source);
            let entity = retain(false, geo, &mut nodes, &mut |nodes, fit_points| {
                source.derive(projection, nodes, fit_points)
            });
            overlay.insert(id.to_owned(), entity);
        }
        for building in &save.scenario.buildings {
            let entity = retain(
                planning.is_suppressed(&building.id),
                None,
                &mut nodes,
                &mut |nodes, fit_points| {
                    append_building_node(building, projection, nodes, fit_points);
                    NodeKind::Area
                },
            );
            scenario.insert(building.id.clone(), entity);
        }
        for water in &save.scenario.water {
            let entity = retain(
                planning.is_suppressed(&water.id),
                None,
                &mut nodes,
                &mut |nodes, fit_points| {
                    append_water_node(water, projection, nodes, fit_points);
                    NodeKind::Area
                },
            );
            scenario.insert(water.id.clone(), entity);
        }
        for land_use in &save.scenario.land_use_areas {
            let entity = retain(
                planning.is_suppressed(&land_use.id),
                None,
                &mut nodes,
                &mut |nodes, fit_points| {
                    append_land_use_node(land_use, projection, nodes, fit_points);
                    NodeKind::Area
                },
            );
            scenario.insert(land_use.id.clone(), entity);
        }
        for (source, geo) in zones {
            let id = overlay_id(source);
            let entity = retain(false, geo, &mut nodes, &mut |nodes, fit_points| {
                source.derive(projection, nodes, fit_points)
            });
            overlay.insert(id.to_owned(), entity);
        }

        nodes.sort_by(|left, right| left.id.cmp(&right.id));
        let unique_node_ids = nodes.windows(2).all(|pair| pair[0].id != pair[1].id);

        let empty_camera = overview_camera_from_extents(overview_eye_distance(0.0), None, aspect)?;
        let mut prepared = Self {
            aspect,
            projection,
            scenario_geo,
            geo,
            scenario,
            overlay,
            max_abs: 0.0,
            eye_distance: overview_eye_distance(0.0),
            extents: None,
            overview: empty_camera,
            frame: RendererFrame {
                camera: renderer_camera(empty_camera, aspect),
                nodes,
            },
            unique_node_ids,
            work,
        };
        prepared.refit_from_summaries(true)?;
        Ok(prepared)
    }

    /// Applies the explicit render impact of one command already executed on `save`.
    pub fn apply(&mut self, save: &CitySave, impact: &CityRenderImpact) -> Result<(), CameraError> {
        self.work = RenderPreparationWork::default();
        if impact.is_unchanged() {
            return Ok(());
        }
        if self.unique_node_ids && matches!(self.apply_incremental(save, impact)?, Step::Applied) {
            return Ok(());
        }
        *self = Self::prepare(save, self.aspect)?;
        Ok(())
    }

    /// Changes the viewport aspect. Only the overview camera is refit; nodes are retained.
    pub fn set_aspect(&mut self, aspect: f32) -> Result<(), CameraError> {
        if !aspect.is_finite() || aspect <= 0.0 {
            return Err(CameraError::InvalidAspect);
        }
        self.work = RenderPreparationWork::default();
        if aspect.to_bits() == self.aspect.to_bits() {
            return Ok(());
        }
        let overview = overview_camera_from_extents(self.eye_distance, self.extents, aspect)?;
        self.aspect = aspect;
        self.set_overview(overview);
        self.work.overview_refits = 1;
        Ok(())
    }

    /// Prepared nodes plus the fitted overview camera.
    #[must_use]
    pub fn frame(&self) -> &RendererFrame {
        &self.frame
    }

    #[must_use]
    pub fn aspect(&self) -> f32 {
        self.aspect
    }

    /// Camera-only inspection relative to the fitted overview. Visits no city entities.
    pub fn camera(&self, view: RenderView) -> Result<RendererCamera, RenderFrameError> {
        let view = view.validate()?;
        let camera = apply_render_view(self.overview, view)?;
        Ok(renderer_camera(camera, self.aspect))
    }

    /// Work counters of the most recent `prepare`, `apply`, or `set_aspect`.
    #[must_use]
    pub fn last_work(&self) -> RenderPreparationWork {
        self.work
    }

    #[cfg(any(test, target_arch = "wasm32"))]
    pub(crate) fn overview_camera(&self) -> OrthographicCamera {
        self.overview
    }

    fn apply_incremental(
        &mut self,
        save: &CitySave,
        impact: &CityRenderImpact,
    ) -> Result<Step, CameraError> {
        // Phase 1: resolve overlay changes and decide whether the projection origin moves,
        // before mutating anything.
        let mut overlay_changes = Vec::new();
        let mut scenario_changes = Vec::new();
        for invalidation in impact.invalidated() {
            match invalidation {
                RenderInvalidation::PlayerEntity(id) => {
                    let source = OverlaySource::resolve(save, id);
                    let geo = source.and_then(OverlaySource::geographic_bounds);
                    overlay_changes.push((id.as_str(), source, geo));
                }
                RenderInvalidation::ScenarioEntity(id) => scenario_changes.push(id.as_str()),
            }
        }
        if !overlay_changes.is_empty() {
            let geo = self.overlay_geo_after(&overlay_changes);
            let projection = GameWorldProjection::from_bounds(geo);
            if !projection.same_origin(self.projection) {
                return Ok(Step::Rebuild);
            }
            self.geo = geo;
        }

        // Phase 2: update retained entities and frame nodes.
        let mut removed_touches_bounds = false;
        let mut added = Vec::new();
        for (id, source, geo) in overlay_changes {
            self.work.entities_visited += 1;
            if let Some(old) = self.overlay.remove(id) {
                removed_touches_bounds |= self.touches_bounds(&old);
                if !self.take_frame_nodes(id, &old, None) {
                    return Ok(Step::Rebuild);
                }
            }
            if let Some(source) = source {
                let mut nodes = Vec::new();
                let mut fit_points = Vec::new();
                let kind = source.derive(self.projection, &mut nodes, &mut fit_points);
                let mut entity = RetainedEntity {
                    kind,
                    node_count: nodes.len(),
                    max_abs: max_abs_of(&fit_points),
                    fit_points,
                    extents: None,
                    hidden: None,
                    geo,
                };
                entity.extents = self.entity_extents(&entity.fit_points)?;
                if !self.insert_frame_nodes(nodes) {
                    return Ok(Step::Rebuild);
                }
                added.push((entity.max_abs, entity.extents));
                self.overlay.insert(id.to_owned(), entity);
            }
        }
        for id in scenario_changes {
            let Some(entity) = self.scenario.get(id) else {
                // Scenario entities without render nodes (transit anchors).
                continue;
            };
            self.work.entities_visited += 1;
            let visible = !save.world.planning.is_suppressed(id);
            if visible == entity.visible() {
                continue;
            }
            let mut entity = self.scenario.remove(id).expect("entity was just found");
            if visible {
                let nodes = entity.hidden.take().unwrap_or_default();
                if !self.insert_frame_nodes(nodes) {
                    return Ok(Step::Rebuild);
                }
                added.push((entity.max_abs, entity.extents));
            } else {
                removed_touches_bounds |= self.touches_bounds(&entity);
                let mut hidden = Vec::with_capacity(entity.node_count);
                if !self.take_frame_nodes(id, &entity, Some(&mut hidden)) {
                    return Ok(Step::Rebuild);
                }
                entity.hidden = Some(hidden);
            }
            self.scenario.insert(id.to_owned(), entity);
        }

        // Phase 3: refit the overview only when the changed geometry can affect it.
        if removed_touches_bounds {
            self.refit_from_summaries(false)?;
            return Ok(Step::Applied);
        }
        let max_abs = added
            .iter()
            .fold(self.max_abs, |extent, (max_abs, _)| extent.max(*max_abs));
        if overview_eye_distance(max_abs).to_bits() != self.eye_distance.to_bits() {
            self.refit_from_summaries(false)?;
            return Ok(Step::Applied);
        }
        self.max_abs = max_abs;
        let extents = added.iter().fold(self.extents, |aggregate, (_, extents)| {
            merge_extents(aggregate, *extents)
        });
        if extents != self.extents {
            let overview = overview_camera_from_extents(self.eye_distance, extents, self.aspect)?;
            self.extents = extents;
            self.set_overview(overview);
            self.work.overview_refits = 1;
        }
        Ok(Step::Applied)
    }

    /// Geographic bounds of scenario + overlay after the given overlay changes.
    fn overlay_geo_after(
        &self,
        changes: &[(&str, Option<OverlaySource<'_>>, Option<GeographicBounds>)],
    ) -> Option<GeographicBounds> {
        let removal_touches = changes.iter().any(|(id, _, _)| {
            self.overlay
                .get(*id)
                .and_then(|entity| entity.geo)
                .zip(self.geo)
                .is_some_and(|(old, aggregate)| old.touches(aggregate))
        });
        if removal_touches {
            // Rescan retained overlay summaries; scenario bounds are immutable.
            let changed = changes.iter().map(|(id, _, _)| *id).collect::<Vec<_>>();
            let retained = self
                .overlay
                .iter()
                .filter(|(id, _)| !changed.contains(&id.as_str()))
                .fold(self.scenario_geo, |bounds, (_, entity)| {
                    merge_geographic(bounds, entity.geo)
                });
            changes.iter().fold(retained, |bounds, (_, _, geo)| {
                merge_geographic(bounds, *geo)
            })
        } else {
            changes.iter().fold(self.geo, |bounds, (_, _, geo)| {
                merge_geographic(bounds, *geo)
            })
        }
    }

    fn touches_bounds(&self, entity: &RetainedEntity) -> bool {
        entity.max_abs == self.max_abs
            || entity
                .extents
                .zip(self.extents)
                .is_some_and(|(extents, aggregate)| extents.touches(aggregate))
    }

    /// Recomputes the aggregate overview from retained per-entity summaries (no save reads and no
    /// node derivation). Per-entity camera-space extents are recomputed only when the eye moves.
    fn refit_from_summaries(&mut self, force_extents: bool) -> Result<(), CameraError> {
        self.work.overview_refits = 1;
        let max_abs = self
            .scenario
            .values()
            .filter(|entity| entity.visible())
            .chain(self.overlay.values())
            .fold(0.0_f32, |extent, entity| extent.max(entity.max_abs));
        let eye_distance = overview_eye_distance(max_abs);
        if force_extents || eye_distance.to_bits() != self.eye_distance.to_bits() {
            let view = overview_orientation(eye_distance)?.view_matrix();
            for entity in self.scenario.values_mut().chain(self.overlay.values_mut()) {
                entity.extents = CameraExtents::of_points(&entity.fit_points, |point| {
                    view.transform_point(point)
                });
            }
        }
        let extents = self
            .scenario
            .values()
            .filter(|entity| entity.visible())
            .chain(self.overlay.values())
            .fold(None, |aggregate, entity| {
                merge_extents(aggregate, entity.extents)
            });
        let overview = overview_camera_from_extents(eye_distance, extents, self.aspect)?;
        self.max_abs = max_abs;
        self.eye_distance = eye_distance;
        self.extents = extents;
        self.set_overview(overview);
        Ok(())
    }

    fn entity_extents(&self, fit_points: &[Vec3]) -> Result<Option<CameraExtents>, CameraError> {
        if fit_points.is_empty() {
            return Ok(None);
        }
        let view = overview_orientation(self.eye_distance)?.view_matrix();
        Ok(CameraExtents::of_points(fit_points, |point| {
            view.transform_point(point)
        }))
    }

    fn set_overview(&mut self, overview: OrthographicCamera) {
        self.overview = overview;
        self.frame.camera = renderer_camera(overview, self.aspect);
    }

    /// Inserts nodes at their sorted positions. Returns false on an id collision, whose relative
    /// order only a full stable sort defines.
    fn insert_frame_nodes(&mut self, nodes: Vec<RendererSceneNode>) -> bool {
        for node in nodes {
            match self
                .frame
                .nodes
                .binary_search_by(|probe| probe.id.as_str().cmp(&node.id))
            {
                Ok(_) => return false,
                Err(position) => {
                    self.frame.nodes.insert(position, node);
                    self.work.nodes_built += 1;
                }
            }
        }
        true
    }

    /// Removes an entity's nodes from the frame, optionally keeping them. Returns false when the
    /// retained bookkeeping and the frame disagree.
    fn take_frame_nodes(
        &mut self,
        entity_id: &str,
        entity: &RetainedEntity,
        mut keep: Option<&mut Vec<RendererSceneNode>>,
    ) -> bool {
        for index in 0..entity.node_count {
            let node_id = entity.node_id(entity_id, index);
            let Ok(position) = self
                .frame
                .nodes
                .binary_search_by(|probe| probe.id.as_str().cmp(&node_id))
            else {
                return false;
            };
            let node = self.frame.nodes.remove(position);
            self.work.nodes_removed += 1;
            if let Some(keep) = keep.as_deref_mut() {
                keep.push(node);
            }
        }
        true
    }
}

fn overlay_id(source: OverlaySource<'_>) -> &str {
    match source {
        OverlaySource::Road(road) => &road.id,
        OverlaySource::Zone(zone) => &zone.id,
    }
}

fn max_abs_of(fit_points: &[Vec3]) -> f32 {
    fit_points.iter().fold(0.0_f32, |extent, point| {
        extent
            .max(point.x.abs())
            .max(point.y.abs())
            .max(point.z.abs())
    })
}

fn merge_extents(
    aggregate: Option<CameraExtents>,
    extents: Option<CameraExtents>,
) -> Option<CameraExtents> {
    match (aggregate, extents) {
        (Some(aggregate), Some(extents)) => Some(aggregate.merge(extents)),
        (aggregate, None) => aggregate,
        (None, extents) => extents,
    }
}

fn merge_geographic(
    bounds: Option<GeographicBounds>,
    other: Option<GeographicBounds>,
) -> Option<GeographicBounds> {
    match (bounds, other) {
        (Some(bounds), Some(other)) => Some(bounds.merge(other)),
        (bounds, None) => bounds,
        (None, other) => other,
    }
}
