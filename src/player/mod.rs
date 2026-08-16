//! 播放后端公共接口。

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

mod backend;
mod spectrum;

pub use self::backend::Player;
