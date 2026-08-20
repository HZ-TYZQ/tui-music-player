//! 应用状态和所有可观察的播放行为。

mod input;
mod media;
mod playback;
mod playlists;

#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{AppConfig, AppPaths};
use crate::library::{LibraryEvent, LibraryWorker};
use crate::media::MediaEvent;
use crate::player::{PlayState, Player, PlayerEvent};
use crate::playlist::PlaylistStore;
use crate::search::SearchIndex;
use crate::spectrum::SpectrumProcessor;
use crate::track::Track;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    Help,
    Playlists,
    PlaylistTracks,
    NameInput,
    DeleteConfirm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BagUpdate {
    Reanchor,
    Leave,
}

pub struct App {
    pub library_dir: PathBuf,
    pub tracks: Vec<Track>,
    /// 时长列显示宽度缓存：仅在新建与 apply_scan_finished 时重算，绘制路径零扫描。
    pub duration_column_width: u16,
    pub selected: usize,
    pub playing_index: Option<usize>,
    pub queue: VecDeque<PathBuf>,
    pub history: Vec<PathBuf>,
    pub player: Player,
    pub should_quit: bool,
    pub message: Option<String>,
    pub scanning: bool,
    pub scan_progress: (usize, usize),
    pub search_active: bool,
    pub search: SearchIndex,
    pub overlay: Overlay,
    pub playlist_selected: usize,
    pub playlist_track_selected: usize,
    pub name_input: String,
    pub playlists: PlaylistStore,
    pub config: AppConfig,
    spectrum: SpectrumProcessor,
    paths: AppPaths,
    library: LibraryWorker,
    rng_state: u64,
    save_config_on_exit: bool,
    pending_selected_path: Option<PathBuf>,
    shuffle_order: Vec<usize>,
    shuffle_cursor: usize,
    media_events: Vec<MediaEvent>,
}

impl App {
    pub fn new(
        library_dir: PathBuf,
        paths: AppPaths,
        config: AppConfig,
        initial_warning: Option<String>,
        save_config_on_exit: bool,
    ) -> Result<Self, String> {
        Self::with_player(
            Player::new()?,
            library_dir,
            paths,
            config,
            initial_warning,
            save_config_on_exit,
        )
    }

    #[cfg(test)]
    fn new_for_tests(
        library_dir: PathBuf,
        paths: AppPaths,
        config: AppConfig,
        initial_warning: Option<String>,
        save_config_on_exit: bool,
    ) -> Result<Self, String> {
        Self::with_player(
            Player::new_for_tests()?,
            library_dir,
            paths,
            config,
            initial_warning,
            save_config_on_exit,
        )
    }

    fn with_player(
        player: Player,
        library_dir: PathBuf,
        paths: AppPaths,
        config: AppConfig,
        initial_warning: Option<String>,
        save_config_on_exit: bool,
    ) -> Result<Self, String> {
        player.set_volume(config.volume);
        player.set_muted(config.muted);
        player.set_spectrum_enabled(config.visualizer_enabled);
        let playlists = PlaylistStore::load(paths.playlists_dir.clone())
            .map_err(|error| format!("无法打开播放列表目录: {error}"))?;
        let playlist_warning = playlists.warnings().first().cloned();
        let library = LibraryWorker::start(library_dir.clone(), paths.cache_db.clone());
        Ok(Self {
            library_dir,
            tracks: Vec::new(),
            duration_column_width: 5,
            selected: 0,
            playing_index: None,
            queue: VecDeque::new(),
            history: Vec::new(),
            player,
            should_quit: false,
            message: initial_warning.or(playlist_warning),
            scanning: false,
            scan_progress: (0, 0),
            search_active: false,
            search: SearchIndex::new(),
            overlay: Overlay::None,
            playlist_selected: 0,
            playlist_track_selected: 0,
            name_input: String::new(),
            playlists,
            config,
            spectrum: SpectrumProcessor::new(),
            paths,
            library,
            rng_state: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
            save_config_on_exit,
            pending_selected_path: None,
            shuffle_order: Vec::new(),
            shuffle_cursor: 0,
            media_events: Vec::new(),
        })
    }

    pub fn visible_indices(&self) -> &[usize] {
        self.search.results()
    }

    pub fn selected_track_index(&self) -> Option<usize> {
        self.visible_indices().get(self.selected).copied()
    }

    pub fn selected_track(&self) -> Option<&Track> {
        self.selected_track_index()
            .and_then(|index| self.tracks.get(index))
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.playing_index.and_then(|index| self.tracks.get(index))
    }

    pub fn on_tick(&mut self) {
        for event in self.library.drain_events() {
            match event {
                LibraryEvent::ScanStarted => {
                    self.scanning = true;
                    self.scan_progress = (0, 0);
                }
                LibraryEvent::Progress { scanned, found } => {
                    self.scan_progress = (scanned, found);
                }
                LibraryEvent::ScanFinished { tracks, warnings } => {
                    self.apply_scan_finished(tracks, warnings);
                }
                LibraryEvent::Warning(warning) => self.message = Some(warning),
                LibraryEvent::Error(error) => {
                    self.scanning = false;
                    self.message = Some(error);
                }
            }
        }

        self.search.tick();
        self.restore_pending_selection();
        self.clamp_selections();

        for event in self.player.drain_events() {
            match event {
                PlayerEvent::EndOfStream => self.play_next(true),
                PlayerEvent::Error(error) => {
                    self.message = Some(format!("播放失败: {error}；正在尝试下一首"));
                    // 播放错误不是自然结束；单曲循环也应先尝试后续歌曲。
                    self.play_next(false);
                }
                PlayerEvent::StateChanged(_) => {}
                PlayerEvent::SpectrumFrame {
                    magnitudes,
                    sample_rate,
                } => {
                    // 逐帧处理：一次 drain 中的多帧都经过完整管线，
                    // 避免 last-wins 丢失瞬态峰值。
                    if self.config.visualizer_enabled {
                        self.spectrum.process_frame(&magnitudes, sample_rate);
                    }
                }
            }
        }

        if self.config.visualizer_enabled && self.player.state() != PlayState::Playing {
            self.spectrum.fade_step();
        }
    }

    /// 当前可视 bar 高度（0.0..=1.0），供 UI 绘制。
    pub fn spectrum_bars(&self) -> &[f32] {
        self.spectrum.bars()
    }

    /// 应用一次完整扫描结果，并按路径尽量保留正在播放与选中的歌曲位置。
    fn apply_scan_finished(&mut self, tracks: Vec<Track>, warnings: Vec<String>) {
        let current_path = self.player.current_path().map(Path::to_path_buf);
        let selected_path = self.selected_track().map(|track| track.path.clone());
        self.tracks = tracks;
        self.duration_column_width = self
            .tracks
            .iter()
            .filter_map(|track| track.duration)
            .map(|duration| crate::ui::fmt_duration(duration).len() as u16)
            .max()
            .unwrap_or(5)
            .max(5);
        self.search.replace_tracks(&self.tracks);
        self.playing_index = current_path
            .as_ref()
            .and_then(|path| self.index_for_path(path));
        if self.config.shuffle {
            if let Some(current) = self.playing_index {
                self.reanchor_shuffle_bag(current);
            } else {
                self.shuffle_order.clear();
                self.shuffle_cursor = 0;
            }
        }
        self.pending_selected_path = selected_path;
        self.restore_pending_selection();
        self.scanning = false;
        self.scan_progress = (self.tracks.len(), self.tracks.len());
        self.message = if warnings.is_empty() {
            Some(format!("扫描完成，共 {} 首歌曲", self.tracks.len()))
        } else {
            Some(format!(
                "扫描完成，共 {} 首歌曲；{} 个文件或目录无法读取",
                self.tracks.len(),
                warnings.len()
            ))
        };
    }

    pub fn save_settings(&mut self) -> Result<(), String> {
        if !self.save_config_on_exit {
            return Ok(());
        }
        self.config.volume = self.player.volume();
        self.config.muted = self.player.is_muted();
        self.config
            .save(&self.paths.config_file)
            .map_err(|error| format!("无法保存配置: {error}"))
    }

    pub(super) fn index_for_path(&self, path: &Path) -> Option<usize> {
        self.tracks.iter().position(|track| track.path == path)
    }
}
