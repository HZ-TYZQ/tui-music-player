//! 应用状态：目录扫描、列表导航、播放控制。

use std::fs;
use std::path::{Path, PathBuf};

use crossterm::event::KeyCode;

use crate::player::Player;

/// 支持的音频扩展名（小写）
pub const AUDIO_EXTS: &[&str] = &["mp3", "flac", "wav", "ogg", "opus", "m4a", "aac"];

pub struct App {
    pub dir: PathBuf,
    pub tracks: Vec<PathBuf>,
    pub selected: usize,
    pub playing_index: Option<usize>,
    pub player: Player,
    pub should_quit: bool,
    /// 状态栏提示信息（如播放失败原因）
    pub message: Option<String>,
}

impl App {
    pub fn new(dir: &Path) -> Self {
        let mut tracks = Vec::new();
        scan_dir(dir, &mut tracks, 0);
        tracks.sort();
        Self {
            dir: dir.to_path_buf(),
            tracks,
            selected: 0,
            playing_index: None,
            player: Player::new(),
            should_quit: false,
            message: None,
        }
    }

    /// 曲目的显示名（相对扫描目录的路径）
    pub fn track_name(&self, idx: usize) -> String {
        let Some(path) = self.tracks.get(idx) else {
            return String::new();
        };
        let rel = path.strip_prefix(&self.dir).unwrap_or(path);
        rel.to_string_lossy().into_owned()
    }

    fn play_index(&mut self, idx: usize) {
        if idx >= self.tracks.len() {
            return;
        }
        // 克隆路径避免借用冲突
        let path = self.tracks[idx].clone();
        match self.player.play(&path) {
            Ok(()) => {
                self.playing_index = Some(idx);
                self.selected = idx;
                self.message = None;
            }
            Err(e) => {
                self.playing_index = None;
                self.message = Some(format!("播放失败: {e}"));
            }
        }
    }

    fn play_selected(&mut self) {
        let idx = self.selected;
        self.play_index(idx);
    }

    /// 手动下一曲（循环）
    fn next(&mut self) {
        if self.tracks.is_empty() {
            return;
        }
        let cur = self.playing_index.unwrap_or(self.selected);
        self.play_index((cur + 1) % self.tracks.len());
    }

    /// 手动上一曲（循环）
    fn prev(&mut self) {
        if self.tracks.is_empty() {
            return;
        }
        let cur = self.playing_index.unwrap_or(self.selected);
        let len = self.tracks.len();
        self.play_index((cur + len - 1) % len);
    }

    fn stop(&mut self) {
        self.player.stop();
        self.playing_index = None;
    }

    fn select_next(&mut self) {
        if !self.tracks.is_empty() {
            self.selected = (self.selected + 1).min(self.tracks.len() - 1);
        }
    }

    fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn handle_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Down | KeyCode::Char('j') => self.select_next(),
            KeyCode::Up | KeyCode::Char('k') => self.select_prev(),
            KeyCode::Enter => self.play_selected(),
            KeyCode::Char(' ') => self.player.toggle_pause(),
            KeyCode::Char('s') => self.stop(),
            KeyCode::Char('n') => self.next(),
            KeyCode::Char('p') => self.prev(),
            _ => {}
        }
    }

    /// 每个 tick 调用：检测曲目自然结束并自动连播（到列表末尾停止）
    pub fn on_tick(&mut self) {
        if self.player.poll_finished()
            && let Some(idx) = self.playing_index
        {
            if idx + 1 < self.tracks.len() {
                self.play_index(idx + 1);
            } else {
                self.playing_index = None;
            }
        }
    }
}

/// 递归扫描目录中的音频文件（限制深度防止意外遍历过深）
fn scan_dir(dir: &Path, out: &mut Vec<PathBuf>, depth: u8) {
    if depth > 8 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_dir() {
            scan_dir(&path, out, depth + 1);
        } else if ft.is_file() && is_audio(&path) {
            out.push(path);
        }
    }
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}
