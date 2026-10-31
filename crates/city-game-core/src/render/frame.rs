use serde::{Deserialize, Serialize};
use three_d_camera::OrthographicCamera;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RendererCamera {
    pub aspect: f32,
    pub view_matrix: [f32; 16],
    pub projection_matrix: [f32; 16],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RendererGeometry {
    Box { size: [f32; 3] },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RendererTransform {
    pub translation: [f32; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_quaternion: Option<[f32; 4]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RendererSceneNode {
    pub id: String,
    pub geometry: RendererGeometry,
    pub color: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
    pub transform: RendererTransform,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RendererFrame {
    pub camera: RendererCamera,
    pub nodes: Vec<RendererSceneNode>,
}

pub(super) fn renderer_frame(
    nodes: Vec<RendererSceneNode>,
    camera: OrthographicCamera,
    aspect: f32,
) -> RendererFrame {
    RendererFrame {
        camera: renderer_camera(camera, aspect),
        nodes,
    }
}

pub(super) fn renderer_camera(camera: OrthographicCamera, aspect: f32) -> RendererCamera {
    RendererCamera {
        aspect,
        view_matrix: camera.view_matrix().elements,
        projection_matrix: camera.projection_matrix().elements,
    }
}
