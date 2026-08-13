//! 独立 JSON 文件保存的命名播放列表。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::atomic_write;

const PLAYLIST_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playlist {
    pub version: u32,
    pub name: String,
    pub tracks: Vec<PathBuf>,
}

impl Playlist {
    fn new(name: String) -> Self {
        Self {
            version: PLAYLIST_VERSION,
            name,
            tracks: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct PlaylistStore {
    directory: PathBuf,
    playlists: Vec<Playlist>,
    warnings: Vec<String>,
}

impl PlaylistStore {
    pub fn load(directory: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&directory)?;
        let mut playlists = Vec::new();
        let mut warnings = Vec::new();
        for entry in fs::read_dir(&directory)? {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    warnings.push(format!("无法读取播放列表目录项: {error}"));
                    continue;
                }
            };
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            match fs::read_to_string(&path).and_then(|json| {
                serde_json::from_str::<Playlist>(&json)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
            }) {
                Ok(playlist) if playlist.version == PLAYLIST_VERSION => playlists.push(playlist),
                Ok(_) => warnings.push(format!("播放列表版本不受支持: {}", path.display())),
                Err(error) => {
                    warnings.push(format!("播放列表 {} 无法读取: {error}", path.display()))
                }
            }
        }
        playlists.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(Self {
            directory,
            playlists,
            warnings,
        })
    }

    pub fn all(&self) -> &[Playlist] {
        &self.playlists
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    pub fn create(&mut self, name: &str) -> Result<usize, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("播放列表名称不能为空".to_owned());
        }
        if self.playlists.iter().any(|playlist| playlist.name == name) {
            return Err(format!("播放列表“{name}”已经存在"));
        }
        let playlist = Playlist::new(name.to_owned());
        self.save_playlist(&playlist)?;
        self.playlists.push(playlist);
        self.playlists
            .sort_by(|left, right| left.name.cmp(&right.name));
        Ok(self
            .playlists
            .iter()
            .position(|playlist| playlist.name == name)
            .unwrap_or_default())
    }

    pub fn add_track(&mut self, index: usize, path: &Path) -> Result<(), String> {
        let playlist = self
            .playlists
            .get_mut(index)
            .ok_or_else(|| "播放列表不存在".to_owned())?;
        if playlist.tracks.iter().any(|item| item == path) {
            return Err("这首歌已经在该播放列表中".to_owned());
        }
        playlist.tracks.push(path.to_path_buf());
        let snapshot = playlist.clone();
        self.save_playlist(&snapshot)
    }

    pub fn remove_track(
        &mut self,
        playlist_index: usize,
        track_index: usize,
    ) -> Result<(), String> {
        let playlist = self
            .playlists
            .get_mut(playlist_index)
            .ok_or_else(|| "播放列表不存在".to_owned())?;
        if track_index >= playlist.tracks.len() {
            return Err("播放列表项目不存在".to_owned());
        }
        playlist.tracks.remove(track_index);
        let snapshot = playlist.clone();
        self.save_playlist(&snapshot)
    }

    pub fn delete(&mut self, index: usize) -> Result<(), String> {
        let playlist = self
            .playlists
            .get(index)
            .ok_or_else(|| "播放列表不存在".to_owned())?;
        let path = self.path_for(&playlist.name);
        fs::remove_file(&path)
            .map_err(|error| format!("无法删除播放列表 {}: {error}", playlist.name))?;
        self.playlists.remove(index);
        Ok(())
    }

    fn save_playlist(&self, playlist: &Playlist) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(playlist)
            .map_err(|error| format!("无法序列化播放列表: {error}"))?;
        atomic_write(&self.path_for(&playlist.name), &json)
            .map_err(|error| format!("无法保存播放列表“{}”: {error}", playlist.name))
    }

    fn path_for(&self, name: &str) -> PathBuf {
        let encoded = name
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        self.directory.join(format!("{encoded}.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_add_reload_and_delete_never_touch_music() {
        let temp = tempfile::tempdir().unwrap();
        let music = temp.path().join("song.flac");
        let missing = temp.path().join("later-missing.flac");
        fs::write(&music, b"not real audio").unwrap();
        fs::write(&missing, b"not real audio").unwrap();
        let directory = temp.path().join("playlists");
        let mut store = PlaylistStore::load(directory.clone()).unwrap();
        let index = store.create("我的/列表").unwrap();
        store.add_track(index, &music).unwrap();
        store.add_track(index, &missing).unwrap();
        assert!(store.add_track(index, &music).is_err());

        fs::remove_file(&missing).unwrap();

        let mut reloaded = PlaylistStore::load(directory).unwrap();
        assert_eq!(
            reloaded.all()[0].tracks,
            vec![music.clone(), missing.clone()]
        );
        reloaded.delete(0).unwrap();
        assert!(music.exists());
        assert!(!missing.exists());
    }

    #[test]
    fn name_conflicts_are_not_overwritten() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = PlaylistStore::load(temp.path().to_path_buf()).unwrap();
        store.create("Favorites").unwrap();
        assert!(store.create("Favorites").is_err());
    }
}
