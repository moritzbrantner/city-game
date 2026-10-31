mod camera;
mod composition;
mod frame;
mod incremental;
#[cfg(any(test, target_arch = "wasm32"))]
mod prepared;
mod projection;
mod scene;
mod view;

pub use composition::{
    build_render_frame, build_render_frame_with_view, build_save_render_frame,
    build_save_render_frame_with_view,
};
pub use frame::{
    RendererCamera, RendererFrame, RendererGeometry, RendererSceneNode, RendererTransform,
};
pub use incremental::{PreparedCityRender, RenderPreparationWork};
pub use view::{RenderFrameError, RenderView, RenderViewError};

#[cfg(any(test, target_arch = "wasm32"))]
pub(crate) use prepared::{PreparedCamera, prepare_save_frame, prepared_camera_view};

pub const THREE_D_LAB_REVISION: &str = "5340bdf31de6761ed16737dd6d19da2431172fd6";
