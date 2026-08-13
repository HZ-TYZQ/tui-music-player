//! XDG 路径和用户配置。

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::track::PlayMode;

const APP_DIR: &str = "tui-music-player";
const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub config_file: PathBuf,
    pub playlists_dir: PathBuf,
    pub cache_db: PathBuf,
    pub default_music_dir: Option<PathBuf>,
}

impl AppPaths {
    pub fn discover() -> io::Result<Self> {
        let config = dirs::config_dir().ok_or_else(|| missing_dir("XDG_CONFIG_HOME"))?;
        let data = dirs::data_dir().ok_or_else(|| missing_dir("XDG_DATA_HOME"))?;
        let cache = dirs::cache_dir().ok_or_else(|| missing_dir("XDG_CACHE_HOME"))?;
        Ok(Self::from_roots(config, data, cache, dirs::audio_dir()))
    }

    pub fn from_roots(
        config: PathBuf,
        data: PathBuf,
        cache: PathBuf,
        music: Option<PathBuf>,
    ) -> Self {
        Self {
            config_file: config.join(APP_DIR).join("config.toml"),
            playlists_dir: data.join(APP_DIR).join("playlists"),
            cache_db: cache.join(APP_DIR).join("library.sqlite3"),
            default_music_dir: music,
        }
    }
}

fn missing_dir(name: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("系统没有提供 {name} 对应的用户目录"),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub version: u32,
    pub library_dir: Option<PathBuf>,
    pub volume: u8,
    pub muted: bool,
    pub play_mode: PlayMode,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            library_dir: None,
            volume: 100,
            muted: false,
            play_mode: PlayMode::Sequential,
        }
    }
}

impl AppConfig {
    pub fn load(path: &Path) -> io::Result<(Self, Option<String>)> {
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok((Self::default(), None));
            }
            Err(error) => return Err(error),
        };

        match toml::from_str::<Self>(&contents) {
            Ok(mut config) => {
                if config.version != CONFIG_VERSION {
                    return Ok((
                        Self::default(),
                        Some(format!(
                            "配置文件 {} 的版本 {} 不受支持（当前版本 {}）",
                            path.display(),
                            config.version,
                            CONFIG_VERSION
                        )),
                    ));
                }
                config.volume = config.volume.min(100);
                Ok((config, None))
            }
            Err(error) => Ok((
                Self::default(),
                Some(format!("配置文件 {} 无法解析: {error}", path.display())),
            )),
        }
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "配置文件路径没有父目录"))?;
        fs::create_dir_all(parent)?;
        let contents = toml::to_string_pretty(self)
            .map_err(|error| io::Error::other(format!("无法序列化配置: {error}")))?;
        atomic_write(path, contents.as_bytes())
    }
}

pub fn atomic_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "目标文件路径没有父目录"))?;
    fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "目标文件名不是 UTF-8"))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(".{file_name}.{}.{}.tmp", std::process::id(), nonce));

    let result = (|| {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trip_and_clamps_volume() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config/config.toml");
        let config = AppConfig {
            volume: 75,
            muted: true,
            play_mode: PlayMode::Shuffle,
            ..AppConfig::default()
        };
        config.save(&path).unwrap();
        let (loaded, warning) = AppConfig::load(&path).unwrap();
        assert!(warning.is_none());
        assert_eq!(loaded.volume, 75);
        assert!(loaded.muted);
        assert_eq!(loaded.play_mode, PlayMode::Shuffle);

        fs::write(&path, "version = 1\nvolume = 255\n").unwrap();
        let (loaded, _) = AppConfig::load(&path).unwrap();
        assert_eq!(loaded.volume, 100);
    }

    #[test]
    fn app_paths_keep_config_data_cache_and_music_separate() {
        let paths = AppPaths::from_roots(
            PathBuf::from("/config"),
            PathBuf::from("/data"),
            PathBuf::from("/cache"),
            Some(PathBuf::from("/music")),
        );
        assert_eq!(
            paths.config_file,
            PathBuf::from("/config/tui-music-player/config.toml")
        );
        assert_eq!(
            paths.playlists_dir,
            PathBuf::from("/data/tui-music-player/playlists")
        );
        assert_eq!(
            paths.cache_db,
            PathBuf::from("/cache/tui-music-player/library.sqlite3")
        );
        assert_eq!(paths.default_music_dir, Some(PathBuf::from("/music")));
    }

    #[test]
    fn invalid_config_is_reported_without_overwrite() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.toml");
        fs::write(&path, "not valid = [").unwrap();
        let (_, warning) = AppConfig::load(&path).unwrap();
        assert!(warning.unwrap().contains("无法解析"));
        assert_eq!(fs::read_to_string(path).unwrap(), "not valid = [");
    }

    #[test]
    fn unsupported_config_version_is_not_silently_rewritten() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.toml");
        fs::write(&path, "version = 99\nvolume = 12\n").unwrap();
        let (config, warning) = AppConfig::load(&path).unwrap();
        assert_eq!(config.volume, 100);
        assert!(warning.unwrap().contains("版本 99 不受支持"));
        assert!(fs::read_to_string(path).unwrap().contains("version = 99"));
    }
}
