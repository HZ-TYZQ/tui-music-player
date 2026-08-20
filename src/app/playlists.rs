//! 命名播放列表弹层的交互和播放行为。

use crossterm::event::KeyCode;

use super::{App, BagUpdate, Overlay};

impl App {
    pub(super) fn handle_overlay_key(&mut self, code: KeyCode) -> bool {
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
}
