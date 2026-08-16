//! Rodio 播放后端骨架。
//!
//! 本文件在迁移期通过 `--features rodio-backend` 启用；Phase B 起再接入完整实现。

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{PlayState, PlayerEvent};

pub struct Player {
    state: PlayState,
    current_path: Option<PathBuf>,
}

impl Player {
    pub fn new() -> Result<Self, String> {
        Err("Rodio 后端尚未实现".to_owned())
    }

    #[doc(hidden)]
    pub fn new_for_tests() -> Result<Self, String> {
        Self::new()
    }

    pub fn state(&self) -> PlayState {
        self.state
    }

    pub fn current_path(&self) -> Option<&Path> {
        self.current_path.as_deref()
    }

    pub fn play(&mut self, _path: &Path) -> Result<(), String> {
        Err("Rodio 后端尚未实现".to_owned())
    }

    pub fn toggle_pause(&mut self) {}

    pub fn stop(&mut self) {
        self.state = PlayState::Stopped;
        self.current_path = None;
    }

    pub fn position(&self) -> Duration {
        Duration::ZERO
    }

    pub fn duration(&self) -> Option<Duration> {
        None
    }

    pub fn seek_relative(&self, _offset_seconds: i64) {}

    pub fn set_volume(&self, _percent: u8) {}

    pub fn volume(&self) -> u8 {
        100
    }

    pub fn set_muted(&self, _muted: bool) {}

    pub fn is_muted(&self) -> bool {
        false
    }

    pub fn set_spectrum_enabled(&self, _enabled: bool) {}

    pub fn drain_events(&mut self) -> Vec<PlayerEvent> {
        Vec::new()
    }
}
