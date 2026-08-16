//! 播放后端公共接口。
//!
//! 默认使用 Rodio。迁移期可通过 `--features gstreamer-backend` 回退到 GStreamer。

pub const SPECTRUM_BANDS: usize = 512;
pub const SPECTRUM_THRESHOLD_DB: f32 = -72.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Playing,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlayerEvent {
    EndOfStream,
    Error(String),
    StateChanged(PlayState),
    SpectrumFrame {
        magnitudes: Vec<f32>,
        sample_rate: u32,
    },
}

#[cfg(feature = "gstreamer-backend")]
mod gstreamer;
#[cfg(not(feature = "gstreamer-backend"))]
mod rodio;
#[cfg(not(feature = "gstreamer-backend"))]
mod spectrum;

#[cfg(feature = "gstreamer-backend")]
pub use self::gstreamer::Player;
#[cfg(not(feature = "gstreamer-backend"))]
pub use self::rodio::Player;
