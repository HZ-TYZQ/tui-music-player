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
pub enum PlayMode {
    #[default]
    Sequential,
    RepeatAll,
    RepeatOne,
    Shuffle,
}

impl PlayMode {
    pub const ALL: [Self; 4] = [
        Self::Sequential,
        Self::RepeatAll,
        Self::RepeatOne,
        Self::Shuffle,
    ];

    pub fn next(self) -> Self {
        match self {
            Self::Sequential => Self::RepeatAll,
            Self::RepeatAll => Self::RepeatOne,
            Self::RepeatOne => Self::Shuffle,
            Self::Shuffle => Self::Sequential,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Sequential => "顺序",
            Self::RepeatAll => "列表循环",
            Self::RepeatOne => "单曲循环",
            Self::Shuffle => "随机",
        }
    }
}
