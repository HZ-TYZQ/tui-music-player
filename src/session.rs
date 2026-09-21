//! 退出时的播放位置，供下次启动恢复。它是可丢弃的状态：
//! 读不出来就当没有，不提示也不阻止启动。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::atomic_write;

const SESSION_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// 保存时的音乐库；换了库就不恢复，避免选中别的库里的同名路径。
    pub library_dir: PathBuf,
    pub track: PathBuf,
    pub position: Duration,
}

#[derive(Serialize, Deserialize)]
struct RawSession {
    version: u32,
    library_dir: PathBuf,
    track: PathBuf,
    position_ms: u64,
}

impl Session {
    pub fn load(path: &Path) -> Option<Self> {
        let contents = fs::read_to_string(path).ok()?;
        let raw: RawSession = toml::from_str(&contents).ok()?;
        (raw.version == SESSION_VERSION).then(|| Self {
            library_dir: raw.library_dir,
            track: raw.track,
            position: Duration::from_millis(raw.position_ms),
        })
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let raw = RawSession {
            version: SESSION_VERSION,
            library_dir: self.library_dir.clone(),
            track: self.track.clone(),
            position_ms: u64::try_from(self.position.as_millis()).unwrap_or(u64::MAX),
        };
        let contents = toml::to_string_pretty(&raw)
            .map_err(|error| io::Error::other(format!("无法序列化会话: {error}")))?;
        atomic_write(path, contents.as_bytes())
    }

    /// 没有可恢复的曲目时删掉旧会话，下次启动就不会回到更早的歌。
    pub fn clear(path: &Path) -> io::Result<()> {
        match fs::remove_file(path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_round_trips_and_clear_is_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state/session.toml");
        let session = Session {
            library_dir: PathBuf::from("/music"),
            track: PathBuf::from("/music/测试 歌曲.flac"),
            position: Duration::from_millis(83_250),
        };
        session.save(&path).unwrap();
        assert_eq!(Session::load(&path), Some(session));

        Session::clear(&path).unwrap();
        assert_eq!(Session::load(&path), None);
        Session::clear(&path).unwrap();
    }

    #[test]
    fn unreadable_or_foreign_session_is_ignored() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.toml");
        fs::write(&path, "not valid = [").unwrap();
        assert_eq!(Session::load(&path), None);

        fs::write(
            &path,
            "version = 99\nlibrary_dir = \"/m\"\ntrack = \"/m/a.wav\"\nposition_ms = 1\n",
        )
        .unwrap();
        assert_eq!(Session::load(&path), None);
    }
}
