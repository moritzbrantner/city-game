//! Disposable, constant-size camera preparation. Never persisted as city state.
use serde::{Deserialize, Serialize};

use super::{
    CameraError, CitySave, OrthographicCamera, RenderFrameError, RenderView, RendererCamera,
    RendererFrame, Vec3, apply_render_view, fit_orthographic_camera, renderer_frame,
    save_render_parts,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PreparedCamera {
    aspect: f32,
    eye: [f32; 3],
    target: [f32; 3],
    up: [f32; 3],
    frustum: [f32; 4],
    depth: [f32; 2],
}

#[derive(Serialize)]
pub(crate) struct PreparedFrame {
    pub frame: RendererFrame,
    pub overview: PreparedCamera,
}

pub(crate) fn prepare_save_frame(
    save: &CitySave,
    aspect: f32,
) -> Result<PreparedFrame, CameraError> {
    if !aspect.is_finite() || aspect <= 0.0 {
        return Err(CameraError::InvalidAspect);
    }
    let (mut nodes, fit_points) = save_render_parts(save);
    nodes.sort_by(|left, right| left.id.cmp(&right.id));
    let camera = fit_orthographic_camera(&fit_points, aspect)?;
    let overview = PreparedCamera {
        aspect,
        eye: [camera.eye.x, camera.eye.y, camera.eye.z],
        target: [camera.target.x, camera.target.y, camera.target.z],
        up: [camera.up.x, camera.up.y, camera.up.z],
        frustum: [camera.left, camera.right, camera.bottom, camera.top],
        depth: [camera.near, camera.far],
    };
    Ok(PreparedFrame {
        frame: renderer_frame(nodes, camera, aspect),
        overview,
    })
}

/// Reuse the fitted overview, not a previous view, so pan/zoom never accumulates drift.
/// This operation cannot visit city entities: its inputs contain only camera data.
pub(crate) fn prepared_camera_view(
    overview: &PreparedCamera,
    view: RenderView,
) -> Result<RendererCamera, RenderFrameError> {
    if !overview.aspect.is_finite() || overview.aspect <= 0.0 {
        return Err(CameraError::InvalidAspect.into());
    }
    let view = view.validate()?;
    let [left, right, bottom, top] = overview.frustum;
    let [near, far] = overview.depth;
    let camera = OrthographicCamera::new(
        Vec3::new(overview.eye[0], overview.eye[1], overview.eye[2]),
        Vec3::new(overview.target[0], overview.target[1], overview.target[2]),
        Vec3::new(overview.up[0], overview.up[1], overview.up[2]),
        left,
        right,
        bottom,
        top,
        near,
        far,
    )?;
    let camera = apply_render_view(camera, view)?;
    Ok(RendererCamera {
        aspect: overview.aspect,
        view_matrix: camera.view_matrix().elements,
        projection_matrix: camera.projection_matrix().elements,
    })
}

#[cfg(test)]
mod tests {
    use crate::{CityCommand, CityScenario, PlannedRoad, PlanningCommand, RoadClass};
    use geo_core::Geometry;

    use super::*;

    fn save() -> CitySave {
        let scenario: CityScenario =
            serde_json::from_str(include_str!("../../../../fixtures/demo-scenario.json")).unwrap();
        CitySave::new(scenario).unwrap()
    }

    fn assert_equivalent(save: &CitySave) {
        let before = serde_json::to_vec(save).unwrap();
        for aspect in [0.75, 1.0, 16.0 / 9.0, 3.0] {
            let prepared = prepare_save_frame(save, aspect).unwrap();
            // Exercise the actual browser JSON round trip, not just Rust values.
            let overview: PreparedCamera =
                serde_json::from_str(&serde_json::to_string(&prepared.overview).unwrap()).unwrap();
            for view in [
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
                RenderView::overview(),
            ] {
                let full =
                    super::super::build_save_render_frame_with_view(save, aspect, view).unwrap();
                assert_eq!(prepared.frame.nodes, full.nodes);
                assert_eq!(prepared_camera_view(&overview, view).unwrap(), full.camera);
            }
        }
        assert_eq!(serde_json::to_vec(save).unwrap(), before);
    }

    #[test]
    fn prepared_camera_matches_full_composition_without_changing_save() {
        assert_equivalent(&save());
    }

    #[test]
    fn prepared_camera_matches_empty_scene() {
        let mut save = save();
        save.scenario.roads.clear();
        save.scenario.buildings.clear();
        save.scenario.water.clear();
        save.scenario.land_use_areas.clear();
        save.scenario.transit_anchors.clear();
        assert_equivalent(&save);
    }

    #[test]
    fn reprepare_observes_outlying_planning_and_restart() {
        let mut save = save();
        let original = prepare_save_frame(&save, 1.0).unwrap();
        save.execute(CityCommand::Planning {
            command: PlanningCommand::AddRoad {
                road: PlannedRoad {
                    id: "player/road/camera-test".to_owned(),
                    geometry: Geometry::LineString {
                        coordinates: vec![[9.0, 49.0], [9.01, 49.01]],
                    },
                    class: RoadClass::Residential,
                    name: None,
                },
            },
        })
        .unwrap();
        let changed = prepare_save_frame(&save, 1.0).unwrap();
        assert_ne!(original.frame, changed.frame);
        assert_equivalent(&save);
        save.execute(CityCommand::Restart).unwrap();
        assert_eq!(
            prepare_save_frame(&save, 1.0).unwrap().frame,
            original.frame
        );
    }

    #[test]
    fn invalid_prepared_camera_and_view_fail_closed() {
        let mut overview = prepare_save_frame(&save(), 1.0).unwrap().overview;
        assert!(
            prepared_camera_view(
                &overview,
                RenderView {
                    pan_x: 0.0,
                    pan_y: 0.0,
                    zoom: 0.0,
                }
            )
            .is_err()
        );
        overview.frustum[1] = overview.frustum[0];
        assert!(prepared_camera_view(&overview, RenderView::overview()).is_err());
        overview.aspect = f32::NAN;
        assert!(prepared_camera_view(&overview, RenderView::overview()).is_err());
        assert!(prepare_save_frame(&save(), f32::INFINITY).is_err());
    }
}
