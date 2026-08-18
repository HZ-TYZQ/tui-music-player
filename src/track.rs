//! 曲目和播放顺序等核心数据模型。

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub path: PathBuf,
    pub relative_path: PathBuf,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration: Option<Duration>,
    pub format: Option<String>,
    pub file_size: u64,
    pub modified_ns: i64,
}

impl Track {
    pub fn display_title(&self) -> &str {
        if self.title.trim().is_empty() {
            self.relative_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("未知曲目")
        } else {
            &self.title
        }
    }

    pub fn searchable_columns(&self) -> [String; 4] {
        [
            self.display_title().to_owned(),
            self.artist.clone().unwrap_or_default(),
            self.album.clone().unwrap_or_default(),
            self.relative_path.to_string_lossy().into_owned(),
        ]
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatMode {
    #[default]
    None,
    All,
    One,
}

impl RepeatMode {
    pub fn next(self) -> Self {
        match self {
            Self::None => Self::All,
            Self::All => Self::One,
            Self::One => Self::None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "顺序",
            Self::All => "列表循环",
            Self::One => "单曲循环",
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackMode {
    pub repeat: RepeatMode,
    pub shuffle: bool,
}

impl PlaybackMode {
    pub fn label(self) -> String {
        if self.shuffle {
            format!("{} · 随机", self.repeat.label())
        } else {
            self.repeat.label().to_owned()
        }
    }
}

/// 仅用于读取 v1.1.0 及更早的 `play_mode` 字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyPlayMode {
    Sequential,
    RepeatAll,
    RepeatOne,
    Shuffle,
}

impl LegacyPlayMode {
    pub fn into_playback_mode(self) -> PlaybackMode {
        match self {
            Self::Sequential => PlaybackMode {
                repeat: RepeatMode::None,
                shuffle: false,
            },
            Self::RepeatAll => PlaybackMode {
                repeat: RepeatMode::All,
                shuffle: false,
            },
            Self::RepeatOne => PlaybackMode {
                repeat: RepeatMode::One,
                shuffle: false,
            },
            Self::Shuffle => PlaybackMode {
                repeat: RepeatMode::None,
                shuffle: true,
            },
        }
    }
}
