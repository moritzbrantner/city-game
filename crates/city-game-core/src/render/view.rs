use core::fmt;

use serde::{Deserialize, Serialize};
use three_d_camera::CameraError;

const MIN_RENDER_ZOOM: f32 = 0.5;
const MAX_RENDER_ZOOM: f32 = 32.0;
const MAX_RENDER_PAN: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderView {
    /// Horizontal viewport-center offset measured in fitted-overview widths.
    pub pan_x: f32,
    /// Vertical viewport-center offset measured in fitted-overview heights.
    pub pan_y: f32,
    /// Multiplicative zoom relative to the fitted overview. `1.0` is the overview.
    pub zoom: f32,
}

impl RenderView {
    #[must_use]
    pub const fn overview() -> Self {
        Self {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
        }
    }

    pub fn validate(self) -> Result<Self, RenderViewError> {
        if !self.pan_x.is_finite() || !self.pan_y.is_finite() {
            return Err(RenderViewError::NonFinitePan);
        }
        if self.pan_x.abs() > MAX_RENDER_PAN || self.pan_y.abs() > MAX_RENDER_PAN {
            return Err(RenderViewError::PanOutOfRange);
        }
        if !self.zoom.is_finite() || !(MIN_RENDER_ZOOM..=MAX_RENDER_ZOOM).contains(&self.zoom) {
            return Err(RenderViewError::ZoomOutOfRange);
        }
        Ok(self)
    }
}

impl Default for RenderView {
    fn default() -> Self {
        Self::overview()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderViewError {
    NonFinitePan,
    PanOutOfRange,
    ZoomOutOfRange,
}

impl fmt::Display for RenderViewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinitePan => formatter.write_str("render-view pan must be finite"),
            Self::PanOutOfRange => {
                formatter.write_str("render-view pan exceeds the supported inspection range")
            }
            Self::ZoomOutOfRange => {
                formatter.write_str("render-view zoom must be finite and between 0.5 and 32")
            }
        }
    }
}

impl std::error::Error for RenderViewError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderFrameError {
    Camera(CameraError),
    View(RenderViewError),
}

impl fmt::Display for RenderFrameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Camera(error) => error.fmt(formatter),
            Self::View(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RenderFrameError {}

impl From<CameraError> for RenderFrameError {
    fn from(error: CameraError) -> Self {
        Self::Camera(error)
    }
}

impl From<RenderViewError> for RenderFrameError {
    fn from(error: RenderViewError) -> Self {
        Self::View(error)
    }
}
