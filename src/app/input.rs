//! 键盘输入、搜索编辑和曲库光标。

use crossterm::event::{KeyCode, KeyEvent};

use super::{App, Overlay};

impl App {
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

    pub(super) fn handle_search_key(&mut self, code: KeyCode) {
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

    pub(super) fn select_next(&mut self) {
        self.cancel_pending_selection_restore();
        if !self.visible_indices().is_empty() {
            self.selected = (self.selected + 1).min(self.visible_indices().len() - 1);
        }
    }

    pub(super) fn select_previous(&mut self) {
        self.cancel_pending_selection_restore();
        self.selected = self.selected.saturating_sub(1);
    }

    pub(super) fn restore_pending_selection(&mut self) {
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

    pub(super) fn clamp_selections(&mut self) {
        self.selected = self
            .selected
            .min(self.visible_indices().len().saturating_sub(1));
        self.playlist_selected = self
            .playlist_selected
            .min(self.playlists.all().len().saturating_sub(1));
    }
}
