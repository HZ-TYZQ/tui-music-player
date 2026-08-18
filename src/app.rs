//! 应用状态和所有可观察的播放行为。

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossterm::event::{KeyCode, KeyEvent};

use crate::config::{AppConfig, AppPaths};
use crate::library::{LibraryEvent, LibraryWorker};
use crate::media::{MediaCommand, MediaEvent};
use crate::player::{PlayState, Player, PlayerEvent};
use crate::playlist::PlaylistStore;
use crate::search::SearchIndex;
use crate::spectrum::SpectrumProcessor;
use crate::track::{PlaybackMode, RepeatMode, Track};

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
enum BagUpdate {
    Reanchor,
    Leave,
}

pub struct App {
    pub library_dir: PathBuf,
    pub tracks: Vec<Track>,
    /// 时长列显示宽度缓存：仅在新与 apply_scan_finished 时重算，绘制路径零扫描。
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
            KeyCode::Char(' ') => self.toggle_or_start(),
            KeyCode::Left | KeyCode::Char('h') => self.seek_rel_micros(-10_000_000),
            KeyCode::Right | KeyCode::Char('l') => self.seek_rel_micros(10_000_000),
            KeyCode::Char('-') => self.change_volume(-5),
            KeyCode::Char('=') | KeyCode::Char('+') => self.change_volume(5),
            KeyCode::Char('m') => self.toggle_mute(),
            KeyCode::Char('n') => self.play_next(false),
            KeyCode::Char('p') => self.play_previous(),
            KeyCode::Char('z') => self.cycle_repeat(),
            KeyCode::Char('s') => self.toggle_shuffle(),
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

    fn play_selected(&mut self) {
        if let Some(index) = self.selected_track_index() {
            self.play_index(index, true, BagUpdate::Reanchor);
        }
    }

    fn play_index(&mut self, index: usize, remember_current: bool, bag: BagUpdate) -> bool {
        let Some(track) = self.tracks.get(index) else {
            return false;
        };
        let path = track.path.clone();
        // 切歌：保留已收敛的 sensitivity，重新进入 fast-adapt（见 spectrum.rs）。
        self.spectrum.on_track_change();
        if remember_current
            && let Some(current) = self.current_track()
            && current.path != path
        {
            self.history.push(current.path.clone());
        }
        let autoplay = self.player.state() != PlayState::Paused;
        let result = if autoplay {
            self.player.play(&path)
        } else {
            self.player.open(&path)
        };
        match result {
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
                match bag {
                    BagUpdate::Reanchor if self.config.shuffle => self.reanchor_shuffle_bag(index),
                    BagUpdate::Reanchor | BagUpdate::Leave => {}
                }
                true
            }
            Err(error) => {
                self.message = Some(format!("播放失败: {error}"));
                false
            }
        }
    }

    fn play_path(&mut self, path: &Path, remember_current: bool, bag: BagUpdate) -> bool {
        self.index_for_path(path)
            .map(|index| self.play_index(index, remember_current, bag))
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
                if self.play_path(&path, true, BagUpdate::Leave) {
                    return;
                }
                continue;
            }
            let Some(next) = self.next_library_index(natural_end) else {
                self.player.stop();
                self.playing_index = None;
                return;
            };
            if self.play_index(next, true, BagUpdate::Leave) {
                return;
            }
        }
        self.player.stop();
        self.playing_index = None;
        self.message = Some("没有可播放的下一首歌曲".to_owned());
    }

    fn next_library_index(&mut self, natural_end: bool) -> Option<usize> {
        let current = self.playing_index.or_else(|| self.selected_track_index());
        if self.config.shuffle {
            if natural_end && self.config.repeat == RepeatMode::One {
                return current;
            }
            return self.next_from_shuffle_bag(current);
        }
        match self.config.repeat {
            RepeatMode::One if natural_end => current,
            RepeatMode::All | RepeatMode::One if !self.tracks.is_empty() => {
                Some((current.unwrap_or(0) + 1) % self.tracks.len())
            }
            RepeatMode::All | RepeatMode::One => None,
            RepeatMode::None => match current {
                Some(index) if index + 1 < self.tracks.len() => Some(index + 1),
                None if !self.tracks.is_empty() => Some(0),
                _ => None,
            },
        }
    }

    fn next_from_shuffle_bag(&mut self, current: Option<usize>) -> Option<usize> {
        if self.tracks.is_empty() {
            return None;
        }
        if self.shuffle_order.is_empty() {
            self.reanchor_shuffle_bag(current.unwrap_or(0));
        }
        if self.shuffle_cursor >= self.shuffle_order.len() {
            match self.config.repeat {
                RepeatMode::None => return None,
                RepeatMode::All | RepeatMode::One => {
                    let avoid = self.shuffle_order.last().copied().or(current).unwrap_or(0);
                    self.reshuffle_round(avoid);
                }
            }
        }
        let next = self.shuffle_order.get(self.shuffle_cursor).copied()?;
        self.shuffle_cursor += 1;
        Some(next)
    }

    fn reanchor_shuffle_bag(&mut self, current: usize) {
        if self.tracks.is_empty() {
            self.shuffle_order.clear();
            self.shuffle_cursor = 0;
            return;
        }
        let current = current.min(self.tracks.len() - 1);
        let mut rest: Vec<usize> = (0..self.tracks.len())
            .filter(|index| *index != current)
            .collect();
        self.shuffle_slice(&mut rest);
        self.shuffle_order = std::iter::once(current).chain(rest).collect();
        self.shuffle_cursor = 1;
    }

    fn reshuffle_round(&mut self, avoid_first: usize) {
        let mut order: Vec<usize> = (0..self.tracks.len()).collect();
        self.shuffle_slice(&mut order);
        if order.len() > 1 && order[0] == avoid_first {
            let swap = (1..order.len())
                .find(|index| order[*index] != avoid_first)
                .unwrap_or(1);
            order.swap(0, swap);
        }
        self.shuffle_order = order;
        self.shuffle_cursor = 0;
    }

    fn shuffle_slice(&mut self, items: &mut [usize]) {
        if items.len() < 2 {
            return;
        }
        for index in (1..items.len()).rev() {
            let other = self.random_index(index + 1);
            items.swap(index, other);
        }
    }

    fn play_previous(&mut self) {
        while let Some(path) = self.history.pop() {
            if self.play_path(&path, false, BagUpdate::Reanchor) {
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

    fn cycle_repeat(&mut self) {
        self.config.repeat = self.config.repeat.next();
        self.message = Some(format!("循环：{}", self.config.repeat.label()));
    }

    fn toggle_shuffle(&mut self) {
        self.config.shuffle = !self.config.shuffle;
        if self.config.shuffle {
            if let Some(current) = self.playing_index.or_else(|| self.selected_track_index()) {
                self.reanchor_shuffle_bag(current);
            }
            self.message = Some("已开启随机播放".to_owned());
        } else {
            self.shuffle_order.clear();
            self.shuffle_cursor = 0;
            self.message = Some("已关闭随机播放".to_owned());
        }
    }

    fn toggle_or_start(&mut self) {
        match self.player.state() {
            PlayState::Playing => self.player.pause(),
            PlayState::Paused => self.player.resume(),
            PlayState::Stopped => {
                self.play_or_resume();
            }
        }
    }

    fn play_or_resume(&mut self) {
        match self.player.state() {
            PlayState::Playing => {}
            PlayState::Paused => self.player.resume(),
            PlayState::Stopped => {
                if let Some(index) = self.playing_index.or_else(|| self.selected_track_index()) {
                    self.play_index(index, false, BagUpdate::Reanchor);
                }
            }
        }
    }

    pub fn apply_media_command(&mut self, command: MediaCommand) {
        match command {
            MediaCommand::Play => self.play_or_resume(),
            MediaCommand::Toggle => self.toggle_or_start(),
            MediaCommand::Pause => self.player.pause(),
            MediaCommand::Next => self.play_next(false),
            MediaCommand::Previous => self.play_previous(),
            MediaCommand::SeekRelMicros(offset) => {
                self.seek_rel_micros(offset);
            }
            MediaCommand::SeekTo { position, track_id } => {
                self.seek_to_requested(position, track_id.as_deref());
            }
            MediaCommand::SetVolume(volume) => {
                if volume > 0 {
                    self.player.set_muted(false);
                    self.config.muted = false;
                }
                self.player.set_volume(volume);
                self.config.volume = volume;
            }
            MediaCommand::SetRepeat(repeat) => self.config.repeat = repeat,
            MediaCommand::SetShuffle(shuffle) => {
                if shuffle != self.config.shuffle {
                    self.toggle_shuffle();
                }
            }
            MediaCommand::Quit => self.should_quit = true,
        }
    }

    fn seek_rel_micros(&mut self, offset: i64) {
        if self.player.state() == PlayState::Stopped {
            return;
        }
        let current = duration_as_micros(self.player.position());
        let requested = current.saturating_add(offset);
        if requested < 0 {
            if self.player.seek_to(Duration::ZERO) {
                self.push_seeked(Duration::ZERO);
            }
            return;
        }
        if let Some(duration) = self.effective_duration()
            && requested > duration_as_micros(duration)
        {
            self.play_next(false);
            return;
        }
        let target = Duration::from_micros(requested as u64);
        if self.player.seek_to(target) {
            self.push_seeked(self.player.position());
        }
    }

    fn seek_to_requested(&mut self, position: Duration, track_id: Option<&str>) {
        if self.player.state() == PlayState::Stopped {
            return;
        }
        if let Some(expected) = track_id
            && expected != self.current_track_id()
        {
            return;
        }
        if let Some(duration) = self.effective_duration()
            && position > duration
        {
            return;
        }
        if self.player.seek_to(position) {
            self.push_seeked(self.player.position());
        }
    }

    fn effective_duration(&self) -> Option<Duration> {
        self.player
            .duration()
            .or_else(|| self.current_track().and_then(|track| track.duration))
    }

    fn push_seeked(&mut self, position: Duration) {
        self.media_events.push(MediaEvent::Seeked { position });
    }

    pub fn drain_media_events(&mut self) -> Vec<MediaEvent> {
        std::mem::take(&mut self.media_events)
    }

    pub fn media_snapshot(&self) -> crate::media::MediaSnapshot {
        let track = self.current_track();
        crate::media::MediaSnapshot {
            status: self.player.state(),
            title: track
                .map(|track| track.display_title().to_owned())
                .unwrap_or_default(),
            artist: track.and_then(|track| track.artist.clone()),
            album: track.and_then(|track| track.album.clone()),
            path: track.map(|track| track.path.clone()),
            duration: self.effective_duration(),
            position: self.player.position(),
            volume: self.player.volume(),
            muted: self.player.is_muted(),
            repeat: self.config.repeat,
            shuffle: self.config.shuffle,
            can_go_previous: !self.history.is_empty(),
            can_go_next: self.can_go_next(),
            track_id: self.current_track_id(),
        }
    }

    fn current_track_id(&self) -> String {
        match self.playing_index {
            Some(index) => format!("/org/mpris/MediaPlayer2/Track/{index}"),
            None => "/org/mpris/MediaPlayer2/TrackList/NoTrack".to_owned(),
        }
    }

    fn can_go_next(&self) -> bool {
        if !self.queue.is_empty() {
            return true;
        }
        if self.tracks.is_empty() {
            return false;
        }
        if self.config.repeat != RepeatMode::None {
            return true;
        }
        if self.config.shuffle {
            if self.shuffle_order.is_empty() {
                return true;
            }
            return self.shuffle_cursor < self.shuffle_order.len();
        }
        match self.playing_index {
            Some(index) => index + 1 < self.tracks.len(),
            None => !self.tracks.is_empty(),
        }
    }

    pub fn playback_mode(&self) -> PlaybackMode {
        PlaybackMode {
            repeat: self.config.repeat,
            shuffle: self.config.shuffle,
        }
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
        self.spectrum.reset_output();
    }

    fn handle_search_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.cancel_pending_selection_restore();
                self.search_active = false;
                self.search.set_query(String::new());
                self.selected = 0;
            }
            KeyCode::Enter => {
                self.cancel_pending_selection_restore();
                self.play_selected();
                self.search_active = false;
            }
            KeyCode::Backspace => {
                self.cancel_pending_selection_restore();
                let mut query = self.search.query().to_owned();
                query.pop();
                self.search.set_query(query);
                self.selected = 0;
            }
            KeyCode::Down => self.select_next(),
            KeyCode::Up => self.select_previous(),
            KeyCode::Char(character) => {
                self.cancel_pending_selection_restore();
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
        if self.play_path(&first, true, BagUpdate::Reanchor) {
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
        self.cancel_pending_selection_restore();
        if !self.visible_indices().is_empty() {
            self.selected = (self.selected + 1).min(self.visible_indices().len() - 1);
        }
    }

    fn select_previous(&mut self) {
        self.cancel_pending_selection_restore();
        self.selected = self.selected.saturating_sub(1);
    }

    fn restore_pending_selection(&mut self) {
        if !self.search.query().is_empty() && self.search.is_running() {
            return;
        }
        let Some(path) = self.pending_selected_path.take() else {
            return;
        };
        self.selected = self
            .index_for_path(&path)
            .and_then(|index| {
                self.visible_indices()
                    .iter()
                    .position(|visible| *visible == index)
            })
            .unwrap_or(0);
    }

    fn cancel_pending_selection_restore(&mut self) {
        self.pending_selected_path = None;
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

fn duration_as_micros(duration: Duration) -> i64 {
    i64::try_from(duration.as_micros()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

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
        let app = App::new_for_tests(music, paths, config, None, false).unwrap();
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

    fn settle_search_and_selection(app: &mut App) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            app.search.tick();
            app.restore_pending_selection();
            if !app.search.is_running() && app.pending_selected_path.is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("搜索和待恢复选择未在截止时间内完成");
    }

    #[test]
    fn playback_modes_choose_expected_library_index() {
        let (_temp, mut app) = test_app(AppConfig::default());
        app.tracks = (0..3)
            .map(|index| track(PathBuf::from(format!("/music/{index}.wav"))))
            .collect();
        app.playing_index = Some(2);

        app.config.repeat = RepeatMode::None;
        app.config.shuffle = false;
        assert_eq!(app.next_library_index(true), None);
        app.config.repeat = RepeatMode::All;
        assert_eq!(app.next_library_index(true), Some(0));
        app.config.repeat = RepeatMode::One;
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
    fn rescan_preserves_selected_track_by_path() {
        let (_temp, mut app) = test_app(AppConfig::default());
        let paths: Vec<PathBuf> = (0..4)
            .map(|index| PathBuf::from(format!("/music/{index}.wav")))
            .collect();
        app.tracks = paths.iter().map(|path| track(path.clone())).collect();
        app.search.replace_tracks(&app.tracks);
        app.selected = 2;

        // 模拟扫描结果顺序打乱：2.wav 挪到最前。
        let mut rescanned: Vec<Track> = paths.iter().map(|path| track(path.clone())).collect();
        rescanned.rotate_left(2);
        app.apply_scan_finished(rescanned, Vec::new());

        assert_eq!(
            app.selected_track().map(|track| track.path.clone()),
            Some(paths[2].clone())
        );
    }

    #[test]
    fn rescan_preserves_selected_track_after_active_search_settles() {
        let (_temp, mut app) = test_app(AppConfig::default());
        let paths: Vec<PathBuf> = (0..4)
            .map(|index| PathBuf::from(format!("/music/{index}.wav")))
            .collect();
        app.tracks = paths.iter().map(|path| track(path.clone())).collect();
        app.search.replace_tracks(&app.tracks);
        app.search.set_query("Song".to_owned());
        settle_search_and_selection(&mut app);
        assert_eq!(app.visible_indices(), &[0, 1, 2, 3]);
        app.selected = 2;

        let mut rescanned: Vec<Track> = paths.iter().map(|path| track(path.clone())).collect();
        // 2.wav 重扫后位于可见结果第 1 项，确保错误回退到第 0 项无法通过测试。
        rescanned.rotate_left(1);
        app.apply_scan_finished(rescanned, Vec::new());
        settle_search_and_selection(&mut app);

        assert_eq!(app.selected, 1);
        assert_eq!(
            app.selected_track().map(|track| track.path.clone()),
            Some(paths[2].clone())
        );
    }

    #[test]
    fn rescan_falls_back_to_first_match_when_selected_track_stops_matching() {
        let (_temp, mut app) = test_app(AppConfig::default());
        let first_path = PathBuf::from("/music/first.wav");
        let selected_path = PathBuf::from("/music/selected.wav");
        let mut first = track(first_path.clone());
        first.title = "Other".to_owned();
        let mut selected = track(selected_path.clone());
        selected.title = "Favorite".to_owned();
        app.tracks = vec![first, selected];
        app.search.replace_tracks(&app.tracks);
        app.search.set_query("Favorite".to_owned());
        settle_search_and_selection(&mut app);
        assert_eq!(
            app.selected_track().map(|track| &track.path),
            Some(&selected_path)
        );

        let mut replacement_match = track(first_path.clone());
        replacement_match.title = "Favorite".to_owned();
        let mut replacement_selected = track(selected_path);
        replacement_selected.title = "Other".to_owned();
        app.apply_scan_finished(vec![replacement_match, replacement_selected], Vec::new());
        settle_search_and_selection(&mut app);

        assert_eq!(app.selected, 0);
        assert_eq!(
            app.selected_track().map(|track| &track.path),
            Some(&first_path)
        );
    }

    #[test]
    fn user_actions_cancel_pending_selection_restore() {
        let (_temp, mut app) = test_app(AppConfig::default());
        app.tracks = (0..3)
            .map(|index| track(PathBuf::from(format!("/music/{index}.wav"))))
            .collect();
        app.search.replace_tracks(&app.tracks);

        app.pending_selected_path = Some(app.tracks[2].path.clone());
        app.select_next();
        assert_eq!(app.selected, 1);
        assert!(app.pending_selected_path.is_none());

        app.pending_selected_path = Some(app.tracks[2].path.clone());
        app.selected = 2;
        app.select_previous();
        assert_eq!(app.selected, 1);
        assert!(app.pending_selected_path.is_none());

        app.pending_selected_path = Some(app.tracks[2].path.clone());
        app.handle_search_key(KeyCode::Char('x'));
        assert!(app.pending_selected_path.is_none());

        app.pending_selected_path = Some(app.tracks[2].path.clone());
        app.handle_search_key(KeyCode::Backspace);
        assert!(app.pending_selected_path.is_none());

        app.pending_selected_path = Some(app.tracks[2].path.clone());
        app.handle_search_key(KeyCode::Esc);
        assert!(app.pending_selected_path.is_none());

        app.pending_selected_path = Some(app.tracks[2].path.clone());
        app.handle_search_key(KeyCode::Enter);
        assert!(app.pending_selected_path.is_none());
    }

    #[test]
    fn rescan_resets_selection_when_selected_track_disappears() {
        let (_temp, mut app) = test_app(AppConfig::default());
        app.tracks = (0..3)
            .map(|index| track(PathBuf::from(format!("/music/{index}.wav"))))
            .collect();
        app.search.replace_tracks(&app.tracks);
        app.selected = 2;

        let replacement = vec![track(PathBuf::from("/music/new.wav"))];
        app.apply_scan_finished(replacement, Vec::new());

        assert_eq!(app.selected, 0);
        assert_eq!(
            app.selected_track().map(|track| track.path.clone()),
            Some(PathBuf::from("/music/new.wav"))
        );
        assert_eq!(app.playing_index, None);
    }

    #[test]
    fn shuffle_does_not_immediately_repeat_current_track() {
        let config = AppConfig {
            shuffle: true,
            ..AppConfig::default()
        };
        let (_temp, mut app) = test_app(config);
        app.tracks = (0..4)
            .map(|index| track(PathBuf::from(format!("/music/{index}.wav"))))
            .collect();
        app.playing_index = Some(2);
        app.reanchor_shuffle_bag(2);
        for _ in 0..3 {
            assert_ne!(app.next_library_index(true), Some(2));
        }
    }

    #[test]
    fn z_cycles_repeat_only_and_s_toggles_shuffle() {
        let (_temp, mut app) = test_app(AppConfig::default());
        assert_eq!(app.config.repeat, RepeatMode::None);
        assert!(!app.config.shuffle);

        app.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));
        assert_eq!(app.config.repeat, RepeatMode::All);
        assert!(!app.config.shuffle);

        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        assert_eq!(app.config.repeat, RepeatMode::All);
        assert!(app.config.shuffle);

        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        assert!(!app.config.shuffle);
        assert_eq!(app.config.repeat, RepeatMode::All);
    }

    #[test]
    fn shuffle_bag_exhausts_then_stops_without_repeat() {
        let config = AppConfig {
            shuffle: true,
            repeat: RepeatMode::None,
            ..AppConfig::default()
        };
        let (_temp, mut app) = test_app(config);
        app.tracks = (0..3)
            .map(|index| track(PathBuf::from(format!("/music/{index}.wav"))))
            .collect();
        app.playing_index = Some(0);
        app.reanchor_shuffle_bag(0);
        let mut seen = vec![0];
        while let Some(next) = app.next_library_index(true) {
            seen.push(next);
        }
        assert_eq!(seen.len(), 3);
        let mut unique = seen.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique, vec![0, 1, 2]);
        assert_eq!(app.next_library_index(true), None);
    }

    #[test]
    fn shuffle_repeat_all_reshuffles_after_bag() {
        let config = AppConfig {
            shuffle: true,
            repeat: RepeatMode::All,
            ..AppConfig::default()
        };
        let (_temp, mut app) = test_app(config);
        app.tracks = (0..3)
            .map(|index| track(PathBuf::from(format!("/music/{index}.wav"))))
            .collect();
        app.playing_index = Some(0);
        app.reanchor_shuffle_bag(0);
        for _ in 0..2 {
            assert!(app.next_library_index(true).is_some());
        }
        let first_of_next_round = app.next_library_index(true);
        assert!(first_of_next_round.is_some());
        assert_eq!(app.shuffle_cursor, 1);
        assert_eq!(app.shuffle_order.len(), 3);
    }

    #[test]
    fn visualizer_toggle_updates_persisted_setting() {
        let (_temp, mut app) = test_app(AppConfig::default());
        assert!(app.config.visualizer_enabled);

        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert!(!app.config.visualizer_enabled);
        assert!(app.spectrum_bars().iter().all(|value| *value == 0.0));

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
    fn duration_column_width_follows_the_longest_track_duration() {
        let (_temp, mut app) = test_app(AppConfig::default());
        assert_eq!(app.duration_column_width, 5);

        app.apply_scan_finished(vec![track(PathBuf::from("/music/a.wav"))], Vec::new());
        assert_eq!(app.duration_column_width, 5);

        let mut long = track(PathBuf::from("/music/long.wav"));
        long.duration = Some(Duration::from_secs(3_661)); // "1:01:01" 宽 7
        app.apply_scan_finished(vec![long], Vec::new());
        assert_eq!(app.duration_column_width, 7);

        app.apply_scan_finished(Vec::new(), Vec::new());
        assert_eq!(app.duration_column_width, 5);
    }
}
