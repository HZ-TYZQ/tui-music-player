//! 应用状态和所有可观察的播放行为。

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crossterm::event::{KeyCode, KeyEvent};

use crate::config::{AppConfig, AppPaths};
use crate::library::{LibraryEvent, LibraryWorker};
use crate::player::{PlayState, Player, PlayerEvent, SPECTRUM_THRESHOLD_DB};
use crate::playlist::PlaylistStore;
use crate::search::SearchIndex;
use crate::track::{PlayMode, Track};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    Help,
    Playlists,
    PlaylistTracks,
    NameInput,
    DeleteConfirm,
}

const SPECTRUM_ATTACK: f32 = 0.65;
const SPECTRUM_DECAY: f32 = 0.18;
const SPECTRUM_EPSILON: f32 = 0.001;
const VISUALIZER_BARS: usize = 32;
const VISUALIZER_MIN_HZ: f32 = 50.0;
const VISUALIZER_MAX_HZ: f32 = 8_000.0;
const VISUALIZER_LOW_FREQUENCY_GAIN: f32 = 0.65;
const VISUALIZER_RESPONSE_GAMMA: f32 = 1.6;

pub struct App {
    pub library_dir: PathBuf,
    pub tracks: Vec<Track>,
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
    pub spectrum: Vec<f32>,
    spectrum_target: Vec<f32>,
    paths: AppPaths,
    library: LibraryWorker,
    rng_state: u64,
    save_config_on_exit: bool,
}

impl App {
    pub fn new(
        library_dir: PathBuf,
        paths: AppPaths,
        config: AppConfig,
        initial_warning: Option<String>,
        save_config_on_exit: bool,
    ) -> Result<Self, String> {
        let player = Player::new()?;
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
            spectrum: vec![0.0; VISUALIZER_BARS],
            spectrum_target: vec![0.0; VISUALIZER_BARS],
            paths,
            library,
            rng_state: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
            save_config_on_exit,
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

    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.handle_overlay_key(key.code) {
            return;
        }
        if self.search_active {
            self.handle_search_key(key.code);
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Down | KeyCode::Char('j') => self.select_next(),
            KeyCode::Up | KeyCode::Char('k') => self.select_previous(),
            KeyCode::Enter => self.play_selected(),
            KeyCode::Char(' ') => self.player.toggle_pause(),
            KeyCode::Left | KeyCode::Char('h') => self.player.seek_relative(-10),
            KeyCode::Right | KeyCode::Char('l') => self.player.seek_relative(10),
            KeyCode::Char('-') => self.change_volume(-5),
            KeyCode::Char('=') | KeyCode::Char('+') => self.change_volume(5),
            KeyCode::Char('m') => self.toggle_mute(),
            KeyCode::Char('n') => self.play_next(false),
            KeyCode::Char('p') => self.play_previous(),
            KeyCode::Char('z') => self.cycle_mode(),
            KeyCode::Char('v') => self.toggle_visualizer(),
            KeyCode::Char('/') => {
                self.search_active = true;
                self.message = None;
            }
            KeyCode::Char('r') => {
                self.library.rescan();
                self.message = Some("已请求重新扫描音乐库".to_owned());
            }
            KeyCode::Char('a') => self.enqueue_selected(false),
            KeyCode::Char('A') => self.enqueue_selected(true),
            KeyCode::Char('P') => self.overlay = Overlay::Playlists,
            KeyCode::Char('?') => self.overlay = Overlay::Help,
            KeyCode::Esc => self.message = None,
            _ => {}
        }
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
                    let current_path = self.player.current_path().map(Path::to_path_buf);
                    self.tracks = tracks;
                    self.search.replace_tracks(&self.tracks);
                    self.selected = 0;
                    self.playing_index = current_path
                        .as_ref()
                        .and_then(|path| self.index_for_path(path));
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
                LibraryEvent::Warning(warning) => self.message = Some(warning),
                LibraryEvent::Error(error) => {
                    self.scanning = false;
                    self.message = Some(error);
                }
            }
        }

        self.search.tick();
        self.clamp_selections();

        for event in self.player.drain_events() {
            match event {
                PlayerEvent::EndOfStream => self.play_next(true),
                PlayerEvent::Error(error) => {
                    self.message = Some(format!("播放失败: {error}；正在尝试下一首"));
                    self.play_next(true);
                }
                PlayerEvent::StateChanged(_) => {}
                PlayerEvent::SpectrumFrame {
                    magnitudes,
                    sample_rate,
                } => {
                    if self.config.visualizer_enabled {
                        self.spectrum_target = map_spectrum_frame(&magnitudes, sample_rate);
                    }
                }
            }
        }

        if self.player.state() != PlayState::Playing {
            self.spectrum_target.fill(0.0);
        }
        if self.config.visualizer_enabled {
            animate_spectrum(&mut self.spectrum, &self.spectrum_target);
        }
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

    fn play_selected(&mut self) {
        if let Some(index) = self.selected_track_index() {
            self.play_index(index, true);
        }
    }

    fn play_index(&mut self, index: usize, remember_current: bool) -> bool {
        let Some(track) = self.tracks.get(index) else {
            return false;
        };
        let path = track.path.clone();
        self.clear_spectrum();
        if remember_current
            && let Some(current) = self.current_track()
            && current.path != path
        {
            self.history.push(current.path.clone());
        }
        match self.player.play(&path) {
            Ok(()) => {
                self.playing_index = Some(index);
                self.message = None;
                if let Some(visible) = self
                    .visible_indices()
                    .iter()
                    .position(|visible_index| *visible_index == index)
                {
                    self.selected = visible;
                }
                true
            }
            Err(error) => {
                self.message = Some(format!("播放失败: {error}"));
                false
            }
        }
    }

    fn play_path(&mut self, path: &Path, remember_current: bool) -> bool {
        self.index_for_path(path)
            .map(|index| self.play_index(index, remember_current))
            .unwrap_or_else(|| {
                self.message = Some(format!("歌曲不存在，已跳过: {}", path.display()));
                false
            })
    }

    fn play_next(&mut self, natural_end: bool) {
        if self.tracks.is_empty() {
            self.playing_index = None;
            return;
        }
        let max_attempts = self.tracks.len().saturating_add(self.queue.len()).max(1);
        for _ in 0..max_attempts {
            if let Some(path) = self.queue.pop_front() {
                if self.play_path(&path, true) {
                    return;
                }
                continue;
            }
            let Some(next) = self.next_library_index(natural_end) else {
                self.player.stop();
                self.playing_index = None;
                return;
            };
            if self.play_index(next, true) {
                return;
            }
        }
        self.player.stop();
        self.playing_index = None;
        self.message = Some("没有可播放的下一首歌曲".to_owned());
    }

    fn next_library_index(&mut self, natural_end: bool) -> Option<usize> {
        let current = self.playing_index.or_else(|| self.selected_track_index());
        match self.config.play_mode {
            PlayMode::RepeatOne if natural_end => current,
            PlayMode::Shuffle => {
                if self.tracks.len() == 1 {
                    Some(0)
                } else {
                    let current = current.unwrap_or(0);
                    let mut next = self.random_index(self.tracks.len());
                    if next == current {
                        next = (next + 1) % self.tracks.len();
                    }
                    Some(next)
                }
            }
            PlayMode::RepeatAll | PlayMode::RepeatOne => {
                Some((current.unwrap_or(0) + 1) % self.tracks.len())
            }
            PlayMode::Sequential => match current {
                Some(index) if index + 1 < self.tracks.len() => Some(index + 1),
                None => Some(0),
                _ => None,
            },
        }
    }

    fn play_previous(&mut self) {
        while let Some(path) = self.history.pop() {
            if self.play_path(&path, false) {
                return;
            }
        }
        self.message = Some("没有上一首播放记录".to_owned());
    }

    fn enqueue_selected(&mut self, next: bool) {
        let Some(path) = self.selected_track().map(|track| track.path.clone()) else {
            return;
        };
        if next {
            self.queue.push_front(path);
            self.message = Some("已设为下一首".to_owned());
        } else {
            self.queue.push_back(path);
            self.message = Some(format!("已加入队列，队列中共 {} 首", self.queue.len()));
        }
    }

    fn change_volume(&mut self, delta: i8) {
        let volume = if delta.is_negative() {
            self.player.volume().saturating_sub(delta.unsigned_abs())
        } else {
            self.player.volume().saturating_add(delta as u8).min(100)
        };
        self.player.set_volume(volume);
        self.config.volume = volume;
        self.message = Some(format!("音量 {volume}%"));
    }

    fn toggle_mute(&mut self) {
        let muted = !self.player.is_muted();
        self.player.set_muted(muted);
        self.config.muted = muted;
        self.message = Some(
            if muted {
                "已静音"
            } else {
                "已取消静音"
            }
            .to_owned(),
        );
    }

    fn cycle_mode(&mut self) {
        self.config.play_mode = self.config.play_mode.next();
        self.message = Some(format!("播放模式：{}", self.config.play_mode.label()));
    }

    fn toggle_visualizer(&mut self) {
        self.config.visualizer_enabled = !self.config.visualizer_enabled;
        self.player
            .set_spectrum_enabled(self.config.visualizer_enabled);
        if self.config.visualizer_enabled {
            self.message = Some("已开启音频频谱".to_owned());
        } else {
            self.clear_spectrum();
            self.message = Some("已关闭音频频谱".to_owned());
        }
    }

    fn clear_spectrum(&mut self) {
        self.spectrum.fill(0.0);
        self.spectrum_target.fill(0.0);
    }

    fn handle_search_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.search_active = false;
                self.search.set_query(String::new());
                self.selected = 0;
            }
            KeyCode::Enter => {
                self.play_selected();
                self.search_active = false;
            }
            KeyCode::Backspace => {
                let mut query = self.search.query().to_owned();
                query.pop();
                self.search.set_query(query);
                self.selected = 0;
            }
            KeyCode::Down => self.select_next(),
            KeyCode::Up => self.select_previous(),
            KeyCode::Char(character) => {
                let mut query = self.search.query().to_owned();
                query.push(character);
                self.search.set_query(query);
                self.selected = 0;
            }
            _ => {}
        }
    }

    fn handle_overlay_key(&mut self, code: KeyCode) -> bool {
        match self.overlay {
            Overlay::None => false,
            Overlay::Help => {
                if matches!(code, KeyCode::Esc | KeyCode::Char('?')) {
                    self.overlay = Overlay::None;
                }
                true
            }
            Overlay::Playlists => {
                match code {
                    KeyCode::Esc | KeyCode::Char('P') => self.overlay = Overlay::None,
                    KeyCode::Down | KeyCode::Char('j') => {
                        if !self.playlists.all().is_empty() {
                            self.playlist_selected =
                                (self.playlist_selected + 1).min(self.playlists.all().len() - 1);
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.playlist_selected = self.playlist_selected.saturating_sub(1);
                    }
                    KeyCode::Char('c') => {
                        self.name_input.clear();
                        self.overlay = Overlay::NameInput;
                    }
                    KeyCode::Char('a') => self.add_selected_to_playlist(),
                    KeyCode::Enter => {
                        if !self.playlists.all().is_empty() {
                            self.playlist_track_selected = 0;
                            self.overlay = Overlay::PlaylistTracks;
                        }
                    }
                    KeyCode::Char('x') if !self.playlists.all().is_empty() => {
                        self.overlay = Overlay::DeleteConfirm;
                    }
                    _ => {}
                }
                true
            }
            Overlay::PlaylistTracks => {
                match code {
                    KeyCode::Esc => self.overlay = Overlay::Playlists,
                    KeyCode::Down | KeyCode::Char('j') => {
                        let len = self
                            .playlists
                            .all()
                            .get(self.playlist_selected)
                            .map(|playlist| playlist.tracks.len())
                            .unwrap_or(0);
                        if len > 0 {
                            self.playlist_track_selected =
                                (self.playlist_track_selected + 1).min(len - 1);
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.playlist_track_selected =
                            self.playlist_track_selected.saturating_sub(1);
                    }
                    KeyCode::Enter => self.play_playlist_from_selected(),
                    KeyCode::Char('d') => self.remove_playlist_track(),
                    _ => {}
                }
                true
            }
            Overlay::NameInput => {
                match code {
                    KeyCode::Esc => self.overlay = Overlay::Playlists,
                    KeyCode::Enter => match self.playlists.create(&self.name_input) {
                        Ok(index) => {
                            self.playlist_selected = index;
                            self.overlay = Overlay::Playlists;
                            self.message = Some("播放列表已创建".to_owned());
                        }
                        Err(error) => self.message = Some(error),
                    },
                    KeyCode::Backspace => {
                        self.name_input.pop();
                    }
                    KeyCode::Char(character) => self.name_input.push(character),
                    _ => {}
                }
                true
            }
            Overlay::DeleteConfirm => {
                match code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        match self.playlists.delete(self.playlist_selected) {
                            Ok(()) => {
                                self.playlist_selected = self.playlist_selected.saturating_sub(1);
                                self.message = Some("播放列表已删除，音乐文件未受影响".to_owned());
                            }
                            Err(error) => self.message = Some(error),
                        }
                        self.overlay = Overlay::Playlists;
                    }
                    KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                        self.overlay = Overlay::Playlists;
                    }
                    _ => {}
                }
                true
            }
        }
    }

    fn add_selected_to_playlist(&mut self) {
        let Some(path) = self.selected_track().map(|track| track.path.clone()) else {
            self.message = Some("没有选中的歌曲".to_owned());
            return;
        };
        match self.playlists.add_track(self.playlist_selected, &path) {
            Ok(()) => self.message = Some("已加入播放列表".to_owned()),
            Err(error) => self.message = Some(error),
        }
    }

    fn play_playlist_from_selected(&mut self) {
        let Some(playlist) = self.playlists.all().get(self.playlist_selected) else {
            return;
        };
        let paths =
            playlist.tracks[self.playlist_track_selected.min(playlist.tracks.len())..].to_vec();
        let mut paths = paths.into_iter();
        let Some(first) = paths.next() else {
            self.message = Some("播放列表是空的".to_owned());
            return;
        };
        self.queue = paths.collect();
        if self.play_path(&first, true) {
            self.overlay = Overlay::None;
        } else {
            self.play_next(false);
            if self.playing_index.is_some() {
                self.overlay = Overlay::None;
            }
        }
    }

    fn remove_playlist_track(&mut self) {
        match self
            .playlists
            .remove_track(self.playlist_selected, self.playlist_track_selected)
        {
            Ok(()) => {
                self.playlist_track_selected = self.playlist_track_selected.saturating_sub(1);
                self.message = Some("已从播放列表移除，音乐文件未受影响".to_owned());
            }
            Err(error) => self.message = Some(error),
        }
    }

    fn select_next(&mut self) {
        if !self.visible_indices().is_empty() {
            self.selected = (self.selected + 1).min(self.visible_indices().len() - 1);
        }
    }

    fn select_previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn clamp_selections(&mut self) {
        self.selected = self
            .selected
            .min(self.visible_indices().len().saturating_sub(1));
        self.playlist_selected = self
            .playlist_selected
            .min(self.playlists.all().len().saturating_sub(1));
    }

    fn index_for_path(&self, path: &Path) -> Option<usize> {
        self.tracks.iter().position(|track| track.path == path)
    }

    fn random_index(&mut self, upper: usize) -> usize {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        (self.rng_state as usize) % upper
    }
}

fn map_spectrum_frame(frame: &[f32], sample_rate: u32) -> Vec<f32> {
    let nyquist = sample_rate as f32 / 2.0;
    let upper_hz = VISUALIZER_MAX_HZ.min(nyquist);
    if frame.is_empty() || upper_hz <= VISUALIZER_MIN_HZ {
        return vec![0.0; VISUALIZER_BARS];
    }

    let bin_width = nyquist / frame.len() as f32;
    let frequency_ratio = upper_hz / VISUALIZER_MIN_HZ;
    (0..VISUALIZER_BARS)
        .map(|bar| {
            let lower_hz =
                VISUALIZER_MIN_HZ * frequency_ratio.powf(bar as f32 / VISUALIZER_BARS as f32);
            let upper_hz =
                VISUALIZER_MIN_HZ * frequency_ratio.powf((bar + 1) as f32 / VISUALIZER_BARS as f32);
            let center_hz = (lower_hz * upper_hz).sqrt();
            let magnitude = interpolate_magnitude(frame, center_hz / bin_width);
            let frequency_position = (bar as f32 + 0.5) / VISUALIZER_BARS as f32;
            let frequency_gain = VISUALIZER_LOW_FREQUENCY_GAIN
                + (1.0 - VISUALIZER_LOW_FREQUENCY_GAIN) * frequency_position;
            normalize_magnitude(magnitude).powf(VISUALIZER_RESPONSE_GAMMA) * frequency_gain
        })
        .collect()
}

fn interpolate_magnitude(frame: &[f32], position: f32) -> f32 {
    let position = position.clamp(0.0, frame.len().saturating_sub(1) as f32);
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    let fraction = position - lower as f32;
    let magnitude = |index: usize| {
        if frame[index].is_finite() {
            frame[index]
        } else {
            SPECTRUM_THRESHOLD_DB
        }
    };
    magnitude(lower) + (magnitude(upper) - magnitude(lower)) * fraction
}

fn normalize_magnitude(magnitude: f32) -> f32 {
    if magnitude.is_finite() {
        ((magnitude.clamp(SPECTRUM_THRESHOLD_DB, 0.0) - SPECTRUM_THRESHOLD_DB)
            / -SPECTRUM_THRESHOLD_DB)
            .clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn animate_spectrum(current: &mut Vec<f32>, target: &[f32]) {
    current.resize(target.len(), 0.0);
    for (current, target) in current.iter_mut().zip(target) {
        let factor = if *target > *current {
            SPECTRUM_ATTACK
        } else {
            SPECTRUM_DECAY
        };
        *current += (*target - *current) * factor;
        if current.abs() < SPECTRUM_EPSILON {
            *current = 0.0;
        }
        *current = current.clamp(0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crossterm::event::KeyModifiers;

    fn test_app(config: AppConfig) -> (tempfile::TempDir, App) {
        let temp = tempfile::tempdir().unwrap();
        let music = temp.path().join("music");
        std::fs::create_dir(&music).unwrap();
        let paths = AppPaths::from_roots(
            temp.path().join("config"),
            temp.path().join("data"),
            temp.path().join("cache"),
            Some(music.clone()),
        );
        let app = App::new(music, paths, config, None, false).unwrap();
        (temp, app)
    }

    fn track(path: PathBuf) -> Track {
        Track {
            relative_path: path.file_name().unwrap().into(),
            path,
            title: "Song".to_owned(),
            artist: None,
            album: None,
            duration: Some(Duration::from_secs(1)),
            format: Some("WAV".to_owned()),
            file_size: 1,
            modified_ns: 1,
        }
    }

    #[test]
    fn playback_modes_choose_expected_library_index() {
        let (_temp, mut app) = test_app(AppConfig::default());
        app.tracks = (0..3)
            .map(|index| track(PathBuf::from(format!("/music/{index}.wav"))))
            .collect();
        app.playing_index = Some(2);

        app.config.play_mode = PlayMode::Sequential;
        assert_eq!(app.next_library_index(true), None);
        app.config.play_mode = PlayMode::RepeatAll;
        assert_eq!(app.next_library_index(true), Some(0));
        app.config.play_mode = PlayMode::RepeatOne;
        assert_eq!(app.next_library_index(true), Some(2));
        assert_eq!(app.next_library_index(false), Some(0));
    }

    #[test]
    fn immediate_play_does_not_clear_manual_queue() {
        let (_temp, mut app) = test_app(AppConfig::default());
        let selected = PathBuf::from("/missing/selected.wav");
        let queued = PathBuf::from("/music/queued.wav");
        app.tracks = vec![track(selected)];
        app.search.replace_tracks(&app.tracks);
        app.queue.push_back(queued.clone());
        app.play_selected();
        assert_eq!(app.queue.front(), Some(&queued));
    }

    #[test]
    fn shuffle_does_not_immediately_repeat_current_track() {
        let config = AppConfig {
            play_mode: PlayMode::Shuffle,
            ..AppConfig::default()
        };
        let (_temp, mut app) = test_app(config);
        app.tracks = (0..4)
            .map(|index| track(PathBuf::from(format!("/music/{index}.wav"))))
            .collect();
        app.playing_index = Some(2);
        for _ in 0..20 {
            assert_ne!(app.next_library_index(true), Some(2));
        }
    }

    #[test]
    fn visualizer_toggle_updates_persisted_setting() {
        let (_temp, mut app) = test_app(AppConfig::default());
        assert!(app.config.visualizer_enabled);

        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert!(!app.config.visualizer_enabled);
        assert!(app.spectrum.iter().all(|value| *value == 0.0));

        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert!(app.config.visualizer_enabled);
    }

    #[test]
    fn visualizer_key_does_not_escape_search_or_overlay_modes() {
        let (_temp, mut app) = test_app(AppConfig::default());
        app.search_active = true;
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert!(app.config.visualizer_enabled);
        assert_eq!(app.search.query(), "v");

        app.search_active = false;
        app.overlay = Overlay::Help;
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert!(app.config.visualizer_enabled);
        assert_eq!(app.overlay, Overlay::Help);
    }

    #[test]
    fn spectrum_normalization_clamps_invalid_and_out_of_range_values() {
        let normalized =
            [-90.0, SPECTRUM_THRESHOLD_DB, -30.0, 0.0, 12.0, f32::NAN].map(normalize_magnitude);
        assert_eq!(normalized, [0.0, 0.0, 0.5, 1.0, 1.0, 0.0]);
    }

    #[test]
    fn spectrum_mapping_uses_logarithmic_50_to_8000_hz_bars() {
        let sample_rate = 48_000;
        let bin_width = sample_rate as f32 / 2.0 / 512.0;
        let mapped_peak = |frequency: f32| {
            let mut frame = vec![SPECTRUM_THRESHOLD_DB; 512];
            frame[(frequency / bin_width).floor() as usize] = 0.0;
            let mapped = map_spectrum_frame(&frame, sample_rate);
            let strongest_bar = mapped
                .iter()
                .enumerate()
                .max_by(|(_, left), (_, right)| left.total_cmp(right))
                .map(|(index, _)| index)
                .unwrap();
            (strongest_bar, mapped)
        };

        let (bass_bar, bass) = mapped_peak(100.0);
        let (treble_bar, _) = mapped_peak(4_000.0);
        assert!(bass_bar < 8);
        assert!(treble_bar > 24);
        assert!(bass_bar < treble_bar);
        assert!(bass.iter().all(|magnitude| *magnitude < 0.75));
        assert_eq!(map_spectrum_frame(&[], sample_rate), vec![0.0; 32]);
    }

    #[test]
    fn spectrum_animation_attacks_quickly_and_decays_without_leaving_bounds() {
        let mut current = vec![0.0, 1.0];
        animate_spectrum(&mut current, &[1.0, 0.0]);
        assert_eq!(current, vec![SPECTRUM_ATTACK, 1.0 - SPECTRUM_DECAY]);
        assert!(current.iter().all(|value| (0.0..=1.0).contains(value)));

        for _ in 0..100 {
            animate_spectrum(&mut current, &[0.0, 0.0]);
        }
        assert_eq!(current, vec![0.0, 0.0]);
    }
}
