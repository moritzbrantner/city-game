use three_d_camera::{CameraError, OrthographicCamera};
use three_d_core::Vec3;

use super::RenderView;

const MIN_VERTICAL_SPAN: f32 = 150.0;
const FRAMING_PADDING: f32 = 1.1;

pub(super) fn apply_render_view(
    overview: OrthographicCamera,
    view: RenderView,
) -> Result<OrthographicCamera, CameraError> {
    let width = overview.right - overview.left;
    let height = overview.top - overview.bottom;
    let center_x = (overview.left + overview.right) * 0.5 + view.pan_x * width;
    let center_y = (overview.bottom + overview.top) * 0.5 + view.pan_y * height;
    let half_width = width * 0.5 / view.zoom;
    let half_height = height * 0.5 / view.zoom;

    OrthographicCamera::new(
        overview.eye,
        overview.target,
        overview.up,
        center_x - half_width,
        center_x + half_width,
        center_y - half_height,
        center_y + half_height,
        overview.near,
        overview.far,
    )
}

pub(super) fn fit_orthographic_camera(
    fit_points: &[Vec3],
    aspect: f32,
) -> Result<OrthographicCamera, CameraError> {
    if !aspect.is_finite() || aspect <= 0.0 {
        return Err(CameraError::InvalidAspect);
    }

    let max_coordinate = fit_points.iter().fold(0.0_f32, |extent, point| {
        extent
            .max(point.x.abs())
            .max(point.y.abs())
            .max(point.z.abs())
    });
    let eye_distance = overview_eye_distance(max_coordinate);

    if fit_points.is_empty() {
        return overview_camera_from_extents(eye_distance, None, aspect);
    }

    let view = overview_orientation(eye_distance)?.view_matrix();
    let extents = CameraExtents::of_points(fit_points, |point| view.transform_point(point));
    overview_camera_from_extents(eye_distance, extents, aspect)
}

/// Overview eye distance from the largest absolute fit-point coordinate.
pub(super) fn overview_eye_distance(max_coordinate: f32) -> f32 {
    (max_coordinate * 2.0).max(100.0)
}

/// Orientation-only camera whose view matrix maps fit points into overview camera space.
pub(super) fn overview_orientation(eye_distance: f32) -> Result<OrthographicCamera, CameraError> {
    OrthographicCamera::new(
        Vec3::new(eye_distance, eye_distance, eye_distance),
        Vec3::ZERO,
        Vec3::new(0.0, 1.0, 0.0),
        -1.0,
        1.0,
        -1.0,
        1.0,
        0.1,
        eye_distance * 8.0,
    )
}

/// Axis-aligned extents of fit points in overview camera space. Min/max are order-independent,
/// so extents merged per entity equal extents folded over the complete point list.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct CameraExtents {
    min: [f32; 3],
    max: [f32; 3],
}

impl CameraExtents {
    pub(super) fn of_points(points: &[Vec3], transform: impl Fn(Vec3) -> Vec3) -> Option<Self> {
        let (first, rest) = points.split_first()?;
        let first = transform(*first);
        let mut extents = Self {
            min: [first.x, first.y, first.z],
            max: [first.x, first.y, first.z],
        };
        for point in rest {
            let point = transform(*point);
            extents.min = [
                extents.min[0].min(point.x),
                extents.min[1].min(point.y),
                extents.min[2].min(point.z),
            ];
            extents.max = [
                extents.max[0].max(point.x),
                extents.max[1].max(point.y),
                extents.max[2].max(point.z),
            ];
        }
        Some(extents)
    }

    pub(super) fn merge(self, other: Self) -> Self {
        Self {
            min: [
                self.min[0].min(other.min[0]),
                self.min[1].min(other.min[1]),
                self.min[2].min(other.min[2]),
            ],
            max: [
                self.max[0].max(other.max[0]),
                self.max[1].max(other.max[1]),
                self.max[2].max(other.max[2]),
            ],
        }
    }

    /// Whether removing `self` from a set whose merged extents are `aggregate` can change them.
    pub(super) fn touches(self, aggregate: Self) -> bool {
        (0..3).any(|axis| {
            self.min[axis] == aggregate.min[axis] || self.max[axis] == aggregate.max[axis]
        })
    }
}

pub(super) fn overview_camera_from_extents(
    eye_distance: f32,
    extents: Option<CameraExtents>,
    aspect: f32,
) -> Result<OrthographicCamera, CameraError> {
    if !aspect.is_finite() || aspect <= 0.0 {
        return Err(CameraError::InvalidAspect);
    }
    let eye = Vec3::new(eye_distance, eye_distance, eye_distance);
    let target = Vec3::ZERO;
    let up = Vec3::new(0.0, 1.0, 0.0);

    let Some(CameraExtents {
        min: [min_x, min_y, min_z],
        max: [max_x, max_y, max_z],
    }) = extents
    else {
        let half_height = MIN_VERTICAL_SPAN * 0.5;
        let half_width = half_height * aspect;
        return OrthographicCamera::new(
            eye,
            target,
            up,
            -half_width,
            half_width,
            -half_height,
            half_height,
            0.1,
            eye_distance * 8.0,
        );
    };

    let center_x = (min_x + max_x) * 0.5;
    let center_y = (min_y + max_y) * 0.5;
    let content_width = (max_x - min_x).max(1.0) * FRAMING_PADDING;
    let content_height = ((max_y - min_y).max(1.0) * FRAMING_PADDING).max(MIN_VERTICAL_SPAN);

    let (frustum_width, frustum_height) = if content_width / content_height > aspect {
        (content_width, content_width / aspect)
    } else {
        (content_height * aspect, content_height)
    };

    let half_width = frustum_width * 0.5;
    let half_height = frustum_height * 0.5;
    let nearest_depth = -max_z;
    let farthest_depth = -min_z;
    let depth_padding = ((farthest_depth - nearest_depth).abs() * 0.1).max(1.0);
    let near = (nearest_depth - depth_padding).max(0.1);
    let far = (farthest_depth + depth_padding).max(near + 1.0);

    OrthographicCamera::new(
        eye,
        target,
        up,
        center_x - half_width,
        center_x + half_width,
        center_y - half_height,
        center_y + half_height,
        near,
        far,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitted_camera_encloses_rotated_wide_scene_and_preserves_aspect() {
        let fit_points = vec![
            Vec3::new(-500.0, 0.0, -500.0),
            Vec3::new(-500.0, 0.0, 500.0),
            Vec3::new(500.0, 0.0, -500.0),
            Vec3::new(500.0, 0.0, 500.0),
            Vec3::new(-500.0, 240.0, -500.0),
            Vec3::new(-500.0, 240.0, 500.0),
            Vec3::new(500.0, 240.0, -500.0),
            Vec3::new(500.0, 240.0, 500.0),
        ];
        let aspect = 0.75;
        let camera = fit_orthographic_camera(&fit_points, aspect).unwrap();
        let view = camera.view_matrix();

        for point in fit_points {
            let camera_point = view.transform_point(point);
            let depth = -camera_point.z;
            assert!(camera_point.x >= camera.left && camera_point.x <= camera.right);
            assert!(camera_point.y >= camera.bottom && camera_point.y <= camera.top);
            assert!(depth >= camera.near && depth <= camera.far);
        }

        let fitted_aspect = (camera.right - camera.left) / (camera.top - camera.bottom);
        assert!((fitted_aspect - aspect).abs() <= 1.0e-5);
    }
}
