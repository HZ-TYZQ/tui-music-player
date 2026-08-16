//! 播放后端公共接口。
//!
//! 默认使用 GStreamer。迁移期可通过 `--features rodio-backend` 切换到 Rodio。

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

#[cfg(not(feature = "rodio-backend"))]
mod gstreamer;
#[cfg(feature = "rodio-backend")]
mod rodio;

#[cfg(not(feature = "rodio-backend"))]
pub use self::gstreamer::Player;
#[cfg(feature = "rodio-backend")]
pub use self::rodio::Player;
